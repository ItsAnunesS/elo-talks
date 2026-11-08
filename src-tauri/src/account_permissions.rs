//! Remote permissions are scoped to WhatsApp; media capture requires user consent.
use gtk::{glib, glib::translate::*, prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use webkit2gtk::{
    PermissionRequestExt, SecurityOrigin, SettingsExt, UserMediaPermissionRequestExt,
    WebContextExt, WebViewExt,
};

fn query_state(name: Option<&str>, trusted: bool) -> webkit2gtk::ffi::WebKitPermissionState {
    match (trusted, name) {
        (true, Some("notifications")) => webkit2gtk::ffi::WEBKIT_PERMISSION_STATE_GRANTED,
        (true, Some("camera" | "microphone")) => webkit2gtk::ffi::WEBKIT_PERMISSION_STATE_PROMPT,
        _ => webkit2gtk::ffi::WEBKIT_PERMISSION_STATE_DENIED,
    }
}

fn trusted_view(view: &webkit2gtk::WebView) -> bool {
    view.uri()
        .and_then(|uri| tauri::Url::parse(&uri).ok())
        .is_some_and(|url| crate::whatsapp::is_origin(&url))
}

fn prompt_for_media(
    view: &webkit2gtk::WebView,
    request: &webkit2gtk::UserMediaPermissionRequest,
    active: Rc<Cell<bool>>,
) {
    let message = match (request.is_for_audio_device(), request.is_for_video_device()) {
        (true, true) => "Allow WhatsApp to use your microphone and camera?",
        (true, false) => "Allow WhatsApp to use your microphone?",
        (false, true) => "Allow WhatsApp to use your camera?",
        (false, false) => {
            request.deny();
            return;
        }
    };
    let Some(parent) = view
        .toplevel()
        .and_then(|widget| widget.downcast::<gtk::Window>().ok())
    else {
        request.deny();
        return;
    };
    let dialog = gtk::MessageDialog::new(
        Some(&parent),
        gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
        gtk::MessageType::Question,
        gtk::ButtonsType::None,
        message,
    );
    dialog.set_secondary_text(Some(
        "Only the devices requested by the active WhatsApp account will be enabled.",
    ));
    dialog.add_buttons(&[
        ("Deny", gtk::ResponseType::Cancel),
        ("Allow", gtk::ResponseType::Accept),
    ]);
    dialog.set_default_response(gtk::ResponseType::Cancel);
    active.set(true);
    let pending = Rc::new(RefCell::new(Some(request.clone())));
    let response_pending = pending.clone();
    let weak_view = view.downgrade();
    dialog.connect_response(move |dialog, response| {
        if let Some(request) = response_pending.borrow_mut().take() {
            let allowed = response == gtk::ResponseType::Accept
                && weak_view
                    .upgrade()
                    .is_some_and(|view| view.is_mapped() && trusted_view(&view));
            if allowed {
                request.allow();
            } else {
                request.deny();
            }
        }
        dialog.close();
    });
    // Navigation, account switching and destruction cancel unanswered requests.
    let weak_dialog = dialog.downgrade();
    let load_handler = view.connect_load_changed(move |_, event| {
        if event == webkit2gtk::LoadEvent::Started {
            if let Some(dialog) = weak_dialog.upgrade() {
                dialog.close();
            }
        }
    });
    let weak_dialog = dialog.downgrade();
    let hide_handler = view.connect_unmap(move |_| {
        if let Some(dialog) = weak_dialog.upgrade() {
            dialog.close();
        }
    });
    let weak_view = view.downgrade();
    let handlers = RefCell::new(Some((load_handler, hide_handler)));
    dialog.connect_destroy(move |_| {
        active.set(false);
        if let Some(request) = pending.borrow_mut().take() {
            request.deny();
        }
        if let (Some(view), Some((load, hide))) =
            (weak_view.upgrade(), handlers.borrow_mut().take())
        {
            view.disconnect(load);
            view.disconnect(hide);
        }
    });
    dialog.show_all();
}

// webkit2gtk 2.0.2 exposes this boxed type through ffi but omits its safe binding.
// Keep references owned by GLib and finish queries synchronously on the GTK thread.
glib::wrapper! {
    struct PermissionStateQuery(Shared<webkit2gtk::ffi::WebKitPermissionStateQuery>);

    match fn {
        ref => |ptr| webkit2gtk::ffi::webkit_permission_state_query_ref(ptr),
        unref => |ptr| webkit2gtk::ffi::webkit_permission_state_query_unref(ptr),
        type_ => || webkit2gtk::ffi::webkit_permission_state_query_get_type(),
    }
}

impl PermissionStateQuery {
    fn resolve(&self) {
        // SAFETY: the boxed query owns a live reference throughout this call.
        // Borrowed strings/origins are converted to owned GLib references before use.
        unsafe {
            let query = self.to_glib_none().0;
            let name: Option<glib::GString> = from_glib_none(
                webkit2gtk::ffi::webkit_permission_state_query_get_name(query),
            );
            let origin: Option<SecurityOrigin> = from_glib_none(
                webkit2gtk::ffi::webkit_permission_state_query_get_security_origin(query),
            );
            let trusted = origin.as_ref().is_some_and(|origin| {
                origin.protocol().as_deref() == Some("https")
                    && origin.host().as_deref() == Some("web.whatsapp.com")
                    && matches!(origin.port(), 0 | 443)
            });
            webkit2gtk::ffi::webkit_permission_state_query_finish(
                query,
                query_state(name.as_deref(), trusted),
            );
        }
    }
}

pub fn attach(view: &webkit2gtk::WebView) {
    if let Some(settings) = WebViewExt::settings(view) {
        settings.set_enable_webrtc(true);
        settings.set_enable_media_stream(true);
    }
    if let Some(context) = view.context() {
        let initialize = |context: &webkit2gtk::WebContext| {
            let origin = SecurityOrigin::for_uri("https://web.whatsapp.com");
            context.initialize_notification_permissions(&[&origin], &[]);
        };
        initialize(&context);
        context.connect_initialize_notification_permissions(initialize);
    }
    // Notification.permission and navigator.permissions.query() use separate
    // WebKit hooks. Leaving this unhandled incorrectly reports "prompt".
    view.connect_local("query-permission-state", false, |values| {
        let Some(query) = values
            .get(1)
            .and_then(|value| value.get::<PermissionStateQuery>().ok())
        else {
            return Some(false.to_value());
        };
        query.resolve();
        Some(true.to_value())
    });
    let media_prompt_active = Rc::new(Cell::new(false));
    view.connect_permission_request(move |view, request| {
        if !trusted_view(view) {
            request.deny();
        } else if request.is::<webkit2gtk::NotificationPermissionRequest>() {
            request.allow();
        } else if let Some(media) = request.downcast_ref::<webkit2gtk::UserMediaPermissionRequest>()
        {
            if view.is_mapped() && !media_prompt_active.get() {
                prompt_for_media(view, media, media_prompt_active.clone());
            } else {
                request.deny();
            }
        } else {
            request.deny();
        }
        true
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_queries_require_consent_and_other_origins_remain_denied() {
        use webkit2gtk::ffi::*;
        for name in ["camera", "microphone"] {
            assert_eq!(
                query_state(Some(name), true),
                WEBKIT_PERMISSION_STATE_PROMPT
            );
            assert_eq!(
                query_state(Some(name), false),
                WEBKIT_PERMISSION_STATE_DENIED
            );
        }
        assert_eq!(
            query_state(Some("notifications"), true),
            WEBKIT_PERMISSION_STATE_GRANTED
        );
        assert_eq!(
            query_state(Some("notifications"), false),
            WEBKIT_PERMISSION_STATE_DENIED
        );
        for name in [
            None,
            Some("geolocation"),
            Some("display-capture"),
            Some("clipboard-read"),
        ] {
            assert_eq!(query_state(name, true), WEBKIT_PERMISSION_STATE_DENIED);
        }
    }
}

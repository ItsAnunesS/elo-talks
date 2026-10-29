//! Notification permission is scoped to the WhatsApp origin in each account context.
use gtk::{glib, glib::translate::*, prelude::*};
use webkit2gtk::{PermissionRequestExt, SecurityOrigin, WebContextExt, WebViewExt};

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
            let allowed = name.as_deref() == Some("notifications")
                && origin.as_ref().is_some_and(|origin| {
                    origin.protocol().as_deref() == Some("https")
                        && origin.host().as_deref() == Some("web.whatsapp.com")
                        && matches!(origin.port(), 0 | 443)
                });
            webkit2gtk::ffi::webkit_permission_state_query_finish(
                query,
                if allowed {
                    webkit2gtk::ffi::WEBKIT_PERMISSION_STATE_GRANTED
                } else {
                    webkit2gtk::ffi::WEBKIT_PERMISSION_STATE_DENIED
                },
            );
        }
    }
}

pub fn attach(view: &webkit2gtk::WebView) {
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
    view.connect_permission_request(|view, request| {
        let allowed = view
            .uri()
            .is_some_and(|uri| uri.starts_with("https://web.whatsapp.com/"))
            && request.is::<webkit2gtk::NotificationPermissionRequest>();
        if allowed {
            request.allow();
        } else {
            request.deny();
        }
        true
    });
}

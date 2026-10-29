//! Notification permission is scoped to the WhatsApp origin in each account context.
use gtk::prelude::*;
use webkit2gtk::{PermissionRequestExt, SecurityOrigin, WebContextExt, WebViewExt};

pub fn attach(view: &webkit2gtk::WebView) {
    if let Some(context) = view.context() {
        let initialize = |context: &webkit2gtk::WebContext| {
            let origin = SecurityOrigin::for_uri("https://web.whatsapp.com");
            context.initialize_notification_permissions(&[&origin], &[]);
        };
        initialize(&context);
        context.connect_initialize_notification_permissions(initialize);
    }
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

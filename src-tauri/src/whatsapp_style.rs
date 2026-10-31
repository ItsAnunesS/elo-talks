//! Presentation only: no script, network access, DOM inspection or remote IPC.
use webkit2gtk::{UserContentManagerExt, WebViewExt};

pub fn attach(view: &webkit2gtk::WebView) {
    if let Some(manager) = view.user_content_manager() {
        manager.add_style_sheet(&webkit2gtk::UserStyleSheet::new(
            include_str!("whatsapp.css"),
            webkit2gtk::UserContentInjectedFrames::TopFrame,
            webkit2gtk::UserStyleLevel::User,
            &["https://web.whatsapp.com/*"],
            &[],
        ));
    }
}

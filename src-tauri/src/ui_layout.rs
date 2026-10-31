use anyhow::{anyhow, Result};
use gtk::prelude::*;
use webkit2gtk::{SettingsExt, WebViewExt};

pub const SIDEBAR_WIDTH: i32 = 304;
pub const HEADER_HEIGHT: i32 = 108;

// Wry's Linux WindowChild uses a GtkBox, where set_position/set_size do not
// establish child bounds. GtkOverlay allocates the shell and account viewport
// natively, including resize and scale changes, without exposing layout IPC.
pub fn attach(view: &webkit2gtk::WebView, account: bool) -> Result<()> {
    let parent = view
        .parent()
        .and_then(|p| p.downcast::<gtk::Box>().ok())
        .ok_or_else(|| anyhow!("GTK container unavailable"))?;
    let overlay = if let Some(overlay) = parent
        .children()
        .into_iter()
        .find_map(|child| child.downcast::<gtk::Overlay>().ok())
    {
        overlay
    } else {
        let overlay = gtk::Overlay::new();
        overlay.set_hexpand(true);
        overlay.set_vexpand(true);
        parent.pack_start(&overlay, true, true, 0);
        overlay.show();
        overlay
    };
    parent.remove(view);
    view.set_hexpand(true);
    view.set_vexpand(true);
    view.set_halign(gtk::Align::Fill);
    view.set_valign(gtk::Align::Fill);
    if account {
        view.set_no_show_all(true);
        view.hide();
        view.set_margin_start(SIDEBAR_WIDTH);
        view.set_margin_top(HEADER_HEIGHT);
        overlay.add_overlay(view);
        // Remote pages may not cover the trusted account and vault controls.
        if let Some(settings) = WebViewExt::settings(view) {
            settings.set_enable_fullscreen(false);
        }
        view.connect_enter_fullscreen(|_| true);
    } else {
        overlay.add(view);
        view.show();
    }
    Ok(())
}

// Wry uses show_all(), which intentionally skips our no_show_all account views.
// Show the selected widget explicitly while keeping sibling accounts hidden.
pub fn set_account_visible(view: &tauri::Webview, visible: bool) -> tauri::Result<()> {
    view.with_webview(move |platform| {
        let widget = platform.inner();
        if visible {
            widget.show();
            widget.grab_focus();
        } else {
            widget.hide();
        }
    })
}

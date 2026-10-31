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
    install_resize_handles(&overlay);
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

// Native input overlays stay above both the trusted shell and remote WebKit views.
// HTML handles alone cannot receive events along an account view's right/bottom edge.
fn install_resize_handles(overlay: &gtk::Overlay) {
    const PREFIX: &str = "elo-resize-";
    let existing: Vec<_> = overlay
        .children()
        .into_iter()
        .filter(|child| child.widget_name().starts_with(PREFIX))
        .collect();
    if !existing.is_empty() {
        for handle in existing {
            overlay.reorder_overlay(&handle, -1);
        }
        return;
    }
    use gtk::{gdk::WindowEdge, Align};
    let edges = [
        (WindowEdge::North, Align::Fill, Align::Start, "n-resize"),
        (WindowEdge::South, Align::Fill, Align::End, "s-resize"),
        (WindowEdge::West, Align::Start, Align::Fill, "w-resize"),
        (WindowEdge::East, Align::End, Align::Fill, "e-resize"),
        (
            WindowEdge::NorthWest,
            Align::Start,
            Align::Start,
            "nw-resize",
        ),
        (WindowEdge::NorthEast, Align::End, Align::Start, "ne-resize"),
        (WindowEdge::SouthWest, Align::Start, Align::End, "sw-resize"),
        (WindowEdge::SouthEast, Align::End, Align::End, "se-resize"),
    ];
    for (edge, horizontal, vertical, cursor) in edges {
        let handle = gtk::EventBox::new();
        handle.set_widget_name(&format!("{PREFIX}{cursor}"));
        handle.set_visible_window(false);
        handle.set_above_child(true);
        handle.set_halign(horizontal);
        handle.set_valign(vertical);
        let corner = horizontal != Align::Fill && vertical != Align::Fill;
        handle.set_size_request(if corner { 12 } else { 5 }, if corner { 12 } else { 5 });
        if horizontal == Align::Fill {
            handle.set_margin_start(12);
            handle.set_margin_end(12);
        }
        if vertical == Align::Fill {
            handle.set_margin_top(12);
            handle.set_margin_bottom(12);
        }
        handle.add_events(
            gtk::gdk::EventMask::BUTTON_PRESS_MASK | gtk::gdk::EventMask::ENTER_NOTIFY_MASK,
        );
        handle.connect_enter_notify_event(move |widget, event| {
            if let Some(window) = event.window() {
                window.set_cursor(gtk::gdk::Cursor::from_name(&widget.display(), cursor).as_ref());
            }
            gtk::glib::Propagation::Proceed
        });
        handle.connect_button_press_event(move |widget, event| {
            if event.button() != 1 {
                return gtk::glib::Propagation::Proceed;
            }
            if let Some(window) = widget
                .toplevel()
                .and_then(|widget| widget.downcast::<gtk::Window>().ok())
            {
                if !window.is_maximized() {
                    let (x, y) = event.root();
                    window.begin_resize_drag(edge, 1, x as i32, y as i32, event.time());
                }
            }
            gtk::glib::Propagation::Stop
        });
        overlay.add_overlay(&handle);
        handle.show();
    }
}

//! Local GTK bootstrap: no browser content, vault credentials or account profiles.
use crate::ipc::{rpc, Request};
use anyhow::Result;
use gtk::{glib, prelude::*};
use serde_json::{json, Value};
use std::{
    cell::Cell,
    path::{Path, PathBuf},
    rc::Rc,
    sync::mpsc,
    time::Duration,
};

fn request(runtime: &Path, operation: &str) -> Result<Value> {
    rpc(
        runtime,
        Request {
            op: operation.into(),
            data: json!({}),
        },
    )
}
#[derive(Clone, Copy)]
enum Action {
    Recover,
    Retry,
    Quit,
}
struct Display {
    title: &'static str,
    text: String,
    attention: bool,
    installing: bool,
    recovery: bool,
    retry: bool,
    fraction: Option<f64>,
}
fn display(status: &Value) -> Display {
    let phase = status["phase"].as_str().unwrap_or("error");
    let text = match phase {
        "checking" => "Checking for updates…",
        "downloading"
            if status["progress"]["step"] == "verifying-download"
                || status["progress"]["step"] == "verifying-release" =>
        {
            "Verifying the update signature…"
        }
        "downloading" => "Downloading the update…",
        "ready" => "Update verified. Preparing installation…",
        "installing" => "Installing the verified update. Please wait…",
        "installed" => "Starting the updated Elo Talks…",
        "error" => status["error"]
            .as_str()
            .unwrap_or("Could not check for updates. Check your connection and try again."),
        "unsupported" => "Open the installed Elo Talks AppImage to check for required updates.",
        _ => "Preparing Elo Talks…",
    };
    let downloaded = status["progress"]["downloaded"].as_f64();
    let total = status["progress"]["total"]
        .as_f64()
        .filter(|total| *total > 0.0);
    let title = match phase {
        "checking" => "Getting things ready",
        "downloading" => "An update is on its way",
        "ready" => "Ready to install",
        "installing" => "Installing your update",
        "installed" => "You’re ready to go",
        "error" => "Update required",
        "unsupported" => "AppImage required",
        _ => "Welcome back",
    };
    let text = if phase == "error" && status["canCheck"] == false {
        match status["retryInSeconds"].as_u64().filter(|wait| *wait > 0) {
            Some(wait) => format!(
                "{text}\n\nYou can try again in {}m {:02}s.",
                wait / 60,
                wait % 60
            ),
            None => text.into(),
        }
    } else {
        text.into()
    };
    Display {
        title,
        text,
        attention: matches!(phase, "error" | "unsupported"),
        installing: matches!(phase, "installing" | "installed"),
        recovery: false,
        retry: phase == "error" && status["canCheck"] != false,
        fraction: downloaded
            .zip(total)
            .map(|(done, total)| (done / total).clamp(0.0, 1.0)),
    }
}
fn can_continue(phase: &str) -> bool {
    phase == "current"
}

pub fn run(runtime: PathBuf) -> Result<()> {
    gtk::init()?;
    let window = gtk::Window::new();
    window.set_title(Some("Elo Talks — Starting"));
    window.set_decorated(false);
    window.set_default_size(500, 400);
    window.set_size_request(460, 380);
    window.set_resizable(true);
    window.set_widget_name("elo-launcher");
    let css = gtk::CssProvider::new();
    css.load_from_string(include_str!("launcher.css"));
    gtk::style_context_add_provider_for_display(
        &gtk::prelude::WidgetExt::display(&window),
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    let layout = gtk::Box::new(gtk::Orientation::Vertical, 24);
    layout.set_margin_top(28);
    layout.set_margin_bottom(24);
    layout.set_margin_start(28);
    layout.set_margin_end(28);

    let brand = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let icon = gtk::gdk_pixbuf::PixbufLoader::with_type("png")?;
    icon.set_size(44, 44);
    icon.write(include_bytes!("../icons/128x128.png"))?;
    icon.close()?;
    let texture = gtk::gdk::Texture::for_pixbuf(&icon.pixbuf().expect("Bundled icon"));
    let mark = gtk::Image::from_paintable(Some(&texture));
    mark.set_size_request(44, 44);
    let identity = gtk::Box::new(gtk::Orientation::Vertical, 3);
    identity.set_hexpand(true);
    let name = gtk::Label::new(Some("Elo Talks"));
    name.set_xalign(0.0);
    name.add_css_class("brand-name");
    let subtitle = gtk::Label::new(Some("Your conversations. Your space."));
    subtitle.set_xalign(0.0);
    subtitle.add_css_class("muted");
    identity.append(&name);
    identity.append(&subtitle);
    brand.append(&mark);
    brand.append(&identity);
    let close = gtk::Button::new();
    close.set_widget_name("launcher-close");
    close.set_icon_name("window-close-symbolic");
    close.update_property(&[gtk::accessible::Property::Label("Close Elo Talks")]);
    close.add_css_class("window-close");
    close.set_valign(gtk::Align::Start);
    brand.append(&close);
    let drag = brand;
    let gesture = gtk::GestureClick::new();
    gesture.set_button(1);
    let parent = window.downgrade();
    gesture.connect_pressed(move |gesture, _, x, y| {
        if let (Some(window), Some(device), Some(event)) = (
            parent.upgrade(),
            gesture.current_event_device(),
            gesture.current_event(),
        ) {
            if let Some(surface) = window.surface().and_downcast::<gtk::gdk::Toplevel>() {
                surface.begin_move(&device, 1, x, y, event.time());
            }
        }
    });
    drag.add_controller(gesture);

    let card = gtk::Box::new(gtk::Orientation::Vertical, 14);
    card.add_css_class("status-card");
    let eyebrow = gtk::Label::new(Some("STARTUP CHECK"));
    eyebrow.set_xalign(0.0);
    eyebrow.add_css_class("eyebrow");
    let title = gtk::Label::new(Some("Welcome back"));
    title.set_xalign(0.0);
    title.set_wrap(true);
    title.add_css_class("status-title");
    let label = gtk::Label::new(Some("Preparing your private space…"));
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.set_max_width_chars(42);
    label.add_css_class("description");
    let progress = gtk::ProgressBar::new();
    progress.set_margin_top(8);
    card.append(&eyebrow);
    card.append(&title);
    card.append(&label);
    card.append(&progress);

    let footer = gtk::Box::new(gtk::Orientation::Vertical, 16);
    let note = gtk::Label::new(Some("Updates are verified before installation."));
    note.set_xalign(0.0);
    note.set_wrap(true);
    note.add_css_class("muted");
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let open = gtk::Button::with_label("Retry vault closure");
    open.add_css_class("primary");
    let quit = gtk::Button::with_label("Quit");
    let retry = gtk::Button::with_label("Try again");
    retry.add_css_class("primary");
    actions.append(&quit);
    actions.append(&open);
    actions.append(&retry);
    footer.append(&note);
    footer.append(&actions);
    layout.append(&drag);
    layout.append(&card);
    layout.append(&footer);
    window.set_child(Some(&layout));
    let (commands, input) = mpsc::channel();
    let (output, updates) = mpsc::channel();
    let parent = window.downgrade();
    close.connect_clicked(move |_| {
        if let Some(window) = parent.upgrade() {
            window.close(); // Uses the same guarded close path as Alt+F4.
        }
    });
    let tx = commands.clone();
    open.connect_clicked(move |button| {
        button.set_sensitive(false);
        let _ = tx.send(Action::Recover);
    });
    let tx = commands.clone();
    retry.connect_clicked(move |button| {
        button.set_sensitive(false);
        let _ = tx.send(Action::Retry);
    });
    let tx = commands.clone();
    quit.connect_clicked(move |button| {
        button.set_sensitive(false);
        let _ = tx.send(Action::Quit);
    });
    let installing = Rc::new(Cell::new(false));
    let closing_guard = installing.clone();
    window.connect_close_request(move |_| {
        if !closing_guard.get() {
            let _ = commands.send(Action::Quit);
        }
        glib::Propagation::Stop
    });
    std::thread::spawn(move || {
        let mut attempted = false;
        let mut operation_failed = false;
        loop {
            let result = (|| -> Result<bool> {
                if let Ok(action) = input.try_recv() {
                    match action {
                        Action::Recover => {
                            request(&runtime, "launcher.recover")?;
                            return Ok(true);
                        }
                        Action::Quit => {
                            request(&runtime, "quit")?;
                            return Ok(true);
                        }
                        Action::Retry => {
                            operation_failed = false;
                            request(&runtime, "update.check")?;
                            attempted = true;
                        }
                    }
                }
                let vault = request(&runtime, "status")?;
                if vault["error"].is_string() {
                    let _ = output.send(Display { title: "Finish locking your vault", attention: true, text: "The vault needs closure recovery. Close any process still using the vault, then retry closure. Updates remain required before opening Elo Talks.".into(), installing: false, recovery: true, retry: false, fraction: None });
                    return Ok(false);
                }
                let status = request(&runtime, "update.status")?;
                let phase = status["phase"].as_str().unwrap_or("error");
                if operation_failed {
                    let _ = output.send(display(&json!({
                        "phase": "error", "canCheck": status["canCheck"]
                    })));
                    return Ok(false);
                }
                let _ = output.send(display(&status));
                if !attempted && phase == "idle" {
                    attempted = true;
                    request(&runtime, "update.check")?;
                } else if can_continue(phase) {
                    request(&runtime, "launcher.continue")?;
                    return Ok(true);
                } else if phase == "available" {
                    request(&runtime, "update.prepare")?;
                } else if phase == "ready" {
                    request(&runtime, "update.install")?;
                    return Ok(true); // Controller replaces this launcher after vault closure.
                }
                Ok(false)
            })();
            match result {
                Ok(true) => break,
                Err(_) => {
                    operation_failed = true;
                    let _ = output.send(display(&json!({"phase":"error", "canCheck": false})));
                }
                _ => {}
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    });
    window.present();
    retry.set_visible(false);
    open.set_visible(false);
    quit.grab_focus();
    let main_loop = glib::MainLoop::new(None, false);
    let completion = main_loop.clone();
    glib::timeout_add_local(Duration::from_millis(100), move || {
        loop {
            let state = match updates.try_recv() {
                Ok(state) => state,
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    completion.quit();
                    return glib::ControlFlow::Break;
                }
            };
            installing.set(state.installing);
            title.set_text(state.title);
            label.set_text(&state.text);
            eyebrow.set_text(if state.attention {
                "NEEDS ATTENTION"
            } else {
                "STARTUP CHECK"
            });
            if state.attention {
                card.add_css_class("attention");
            } else {
                card.remove_css_class("attention");
            }
            progress.set_visible(!state.attention);
            note.set_text(if state.installing {
                "Please keep Elo Talks open until installation finishes."
            } else {
                "Updates are verified before installation."
            });
            quit.set_sensitive(!state.installing);
            close.set_sensitive(!state.installing);
            open.set_visible(state.recovery);
            open.set_sensitive(state.recovery);
            retry.set_sensitive(state.retry);
            retry.set_visible(state.retry);
            if let Some(value) = state.fraction {
                progress.set_fraction(value);
            } else if state.installing {
                progress.pulse();
            } else {
                progress.set_fraction(0.0);
            }
        }
        glib::ControlFlow::Continue
    });
    main_loop.run();
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_a_confirmed_current_version_can_open() {
        for phase in ["installing", "installed"] {
            assert!(!can_continue(phase));
            assert!(display(&json!({"phase":phase})).installing);
        }
        for phase in [
            "error",
            "checking",
            "downloading",
            "ready",
            "unsupported",
            "idle",
            "available",
        ] {
            assert!(!can_continue(phase));
        }
        assert!(can_continue("current"));
    }
    #[test]
    fn progress_is_bounded_and_errors_respect_cooldown() {
        let state =
            display(&json!({"phase":"downloading", "progress":{"downloaded":200,"total":100}}));
        assert_eq!(state.fraction, Some(1.0));
        assert!(!display(&json!({"phase":"error", "canCheck":false})).retry);
        let waiting = display(&json!({"phase":"error", "canCheck":false,
            "error":"GitHub has temporarily limited update requests.", "retryInSeconds":125}));
        assert!(waiting.text.contains("GitHub has temporarily limited"));
        assert!(waiting.text.contains("2m 05s"));
        let retry = display(&json!({"phase":"error", "canCheck":true, "retryInSeconds":0}));
        assert!(retry.retry);
        assert!(!retry.text.contains("You can try again in"));
    }
}

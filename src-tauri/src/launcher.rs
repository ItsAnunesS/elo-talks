//! Local GTK bootstrap: no WebKit, vault credentials or account profiles.
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
        "error" if status["canCheck"] == false => "The update could not finish. Please wait before trying again, or close Elo Talks and reopen it later.",
        "error" => "An update check is required before opening Elo Talks. Check your connection and try again.",
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
    Display {
        title,
        text: text.into(),
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
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("Elo Talks — Starting");
    window.set_default_size(500, 400);
    window.set_size_request(460, 380);
    window.set_resizable(true);
    window.set_position(gtk::WindowPosition::Center);
    window.set_widget_name("elo-launcher");
    let css = gtk::CssProvider::new();
    css.load_from_data(include_bytes!("launcher.css"))?;
    gtk::StyleContext::add_provider_for_screen(
        &gtk::prelude::WidgetExt::screen(&window).expect("GTK window has a screen"),
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
    let mark = gtk::Image::from_pixbuf(icon.pixbuf().as_ref());
    mark.set_size_request(44, 44);
    let identity = gtk::Box::new(gtk::Orientation::Vertical, 3);
    let name = gtk::Label::new(Some("Elo Talks"));
    name.set_xalign(0.0);
    name.style_context().add_class("brand-name");
    let subtitle = gtk::Label::new(Some("Your conversations. Your space."));
    subtitle.set_xalign(0.0);
    subtitle.style_context().add_class("muted");
    identity.pack_start(&name, false, false, 0);
    identity.pack_start(&subtitle, false, false, 0);
    brand.pack_start(&mark, false, false, 0);
    brand.pack_start(&identity, true, true, 0);

    let card = gtk::Box::new(gtk::Orientation::Vertical, 14);
    card.style_context().add_class("status-card");
    let eyebrow = gtk::Label::new(Some("STARTUP CHECK"));
    eyebrow.set_xalign(0.0);
    eyebrow.style_context().add_class("eyebrow");
    let title = gtk::Label::new(Some("Welcome back"));
    title.set_xalign(0.0);
    title.set_line_wrap(true);
    title.style_context().add_class("status-title");
    let label = gtk::Label::new(Some("Preparing your private space…"));
    label.set_xalign(0.0);
    label.set_line_wrap(true);
    label.set_max_width_chars(42);
    label.style_context().add_class("description");
    let progress = gtk::ProgressBar::new();
    progress.set_margin_top(8);
    card.pack_start(&eyebrow, false, false, 0);
    card.pack_start(&title, false, false, 0);
    card.pack_start(&label, true, true, 0);
    card.pack_start(&progress, false, false, 0);

    let footer = gtk::Box::new(gtk::Orientation::Vertical, 16);
    let note = gtk::Label::new(Some("Updates are verified before installation."));
    note.set_xalign(0.0);
    note.set_line_wrap(true);
    note.style_context().add_class("muted");
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let open = gtk::Button::with_label("Retry vault closure");
    open.style_context().add_class("primary");
    let quit = gtk::Button::with_label("Quit");
    let retry = gtk::Button::with_label("Try again");
    retry.style_context().add_class("primary");
    actions.pack_start(&quit, false, false, 0);
    actions.pack_start(&open, true, true, 0);
    actions.pack_start(&retry, true, true, 0);
    footer.pack_start(&note, false, false, 0);
    footer.pack_start(&actions, false, false, 0);
    layout.pack_start(&brand, false, false, 0);
    layout.pack_start(&card, true, true, 0);
    layout.pack_start(&footer, false, false, 0);
    window.add(&layout);
    let (commands, input) = mpsc::channel();
    let (output, updates) = mpsc::channel();
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
    window.connect_delete_event(move |_, _| {
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
    window.show_all();
    retry.hide();
    open.hide();
    quit.grab_focus();
    glib::timeout_add_local(Duration::from_millis(100), move || {
        while let Ok(state) = updates.try_recv() {
            installing.set(state.installing);
            title.set_text(state.title);
            label.set_text(&state.text);
            eyebrow.set_text(if state.attention {
                "NEEDS ATTENTION"
            } else {
                "STARTUP CHECK"
            });
            if state.attention {
                card.style_context().add_class("attention");
            } else {
                card.style_context().remove_class("attention");
            }
            progress.set_visible(!state.attention);
            note.set_text(if state.installing {
                "Please keep Elo Talks open until installation finishes."
            } else {
                "Updates are verified before installation."
            });
            quit.set_sensitive(!state.installing);
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
    gtk::main();
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
    }
}

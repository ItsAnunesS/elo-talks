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
    time::{Duration, Instant},
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
    Continue,
    Retry,
    Quit,
}
struct Display {
    text: String,
    installing: bool,
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
        "error" => "The update could not finish. You can open the current version or try again.",
        _ => "Preparing Elo Talks…",
    };
    let downloaded = status["progress"]["downloaded"].as_f64();
    let total = status["progress"]["total"]
        .as_f64()
        .filter(|total| *total > 0.0);
    Display {
        text: text.into(),
        installing: !can_continue(phase),
        retry: phase == "error" && status["canCheck"] != false,
        fraction: downloaded
            .zip(total)
            .map(|(done, total)| (done / total).clamp(0.0, 1.0)),
    }
}
fn can_continue(phase: &str) -> bool {
    !matches!(phase, "installing" | "installed")
}

pub fn run(runtime: PathBuf) -> Result<()> {
    gtk::init()?;
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("Elo Talks — Starting");
    window.set_default_size(460, 260);
    window.set_resizable(false);
    window.set_position(gtk::WindowPosition::Center);
    let layout = gtk::Box::new(gtk::Orientation::Vertical, 18);
    layout.set_margin_top(28);
    layout.set_margin_bottom(28);
    layout.set_margin_start(28);
    layout.set_margin_end(28);
    let title = gtk::Label::new(Some("Elo Talks"));
    title.style_context().add_class("title");
    let label = gtk::Label::new(Some("Preparing your private space…"));
    label.set_line_wrap(true);
    label.set_max_width_chars(48);
    let progress = gtk::ProgressBar::new();
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let open = gtk::Button::with_label("Open current version");
    let retry = gtk::Button::with_label("Try again");
    actions.pack_start(&open, true, true, 0);
    actions.pack_start(&retry, false, false, 0);
    layout.pack_start(&title, false, false, 0);
    layout.pack_start(&label, true, true, 0);
    layout.pack_start(&progress, false, false, 0);
    layout.pack_start(&actions, false, false, 0);
    window.add(&layout);
    let (commands, input) = mpsc::channel();
    let (output, updates) = mpsc::channel();
    let tx = commands.clone();
    open.connect_clicked(move |button| {
        button.set_sensitive(false);
        let _ = tx.send(Action::Continue);
    });
    let tx = commands.clone();
    retry.connect_clicked(move |button| {
        button.set_sensitive(false);
        let _ = tx.send(Action::Retry);
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
        let mut paused = false;
        let mut started = Instant::now();
        loop {
            let result = (|| -> Result<bool> {
                if let Ok(action) = input.try_recv() {
                    match action {
                        Action::Continue => {
                            request(&runtime, "launcher.continue")?;
                            return Ok(true);
                        }
                        Action::Quit => {
                            request(&runtime, "quit")?;
                            return Ok(true);
                        }
                        Action::Retry => {
                            request(&runtime, "update.check")?;
                            paused = false;
                            attempted = true;
                            started = Instant::now();
                        }
                    }
                }
                if paused {
                    return Ok(false);
                }
                let vault = request(&runtime, "status")?;
                if vault["error"].is_string() {
                    paused = true;
                    let _ = output.send(Display { text: "The vault needs closure recovery. Open the current version to finish locking it safely.".into(), installing: false, retry: false, fraction: None });
                    return Ok(false);
                }
                let status = request(&runtime, "update.status")?;
                let phase = status["phase"].as_str().unwrap_or("error");
                let _ = output.send(display(&status));
                if !attempted && phase == "idle" {
                    attempted = true;
                    request(&runtime, "update.check")?;
                } else if matches!(phase, "current" | "unsupported") {
                    request(&runtime, "launcher.continue")?;
                    return Ok(true);
                } else if phase == "available" {
                    request(&runtime, "update.prepare")?;
                } else if phase == "ready" {
                    request(&runtime, "update.install")?;
                    return Ok(true); // Controller replaces this launcher after vault closure.
                } else if phase == "error"
                    || (started.elapsed() > Duration::from_secs(20) && phase == "checking")
                {
                    paused = true;
                    let _ = output.send(display(
                        &json!({"phase":"error", "canCheck": status["canCheck"]}),
                    ));
                }
                Ok(false)
            })();
            match result {
                Ok(true) => break,
                Err(_) => {
                    paused = true;
                    let _ = output.send(display(&json!({"phase":"error", "canCheck": false})));
                }
                _ => {}
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    });
    window.show_all();
    retry.hide();
    open.grab_focus();
    glib::timeout_add_local(Duration::from_millis(100), move || {
        while let Ok(state) = updates.try_recv() {
            installing.set(state.installing);
            label.set_text(&state.text);
            open.set_sensitive(!state.installing);
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
    fn installation_never_offers_current_version() {
        for phase in ["installing", "installed"] {
            assert!(!can_continue(phase));
            assert!(display(&json!({"phase":phase})).installing);
        }
        for phase in ["error", "checking", "downloading", "ready"] {
            assert!(can_continue(phase));
        }
    }
    #[test]
    fn progress_is_bounded_and_errors_respect_cooldown() {
        let state =
            display(&json!({"phase":"downloading", "progress":{"downloaded":200,"total":100}}));
        assert_eq!(state.fraction, Some(1.0));
        assert!(!display(&json!({"phase":"error", "canCheck":false})).retry);
    }
}

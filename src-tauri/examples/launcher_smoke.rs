//! Isolated GTK bootstrap smoke test with synthetic IPC only. Run under Xvfb.
#[path = "../src/ipc.rs"]
mod ipc;
#[path = "../src/launcher.rs"]
mod launcher;
use gtk::prelude::*;
use serde_json::json;
use std::{
    os::unix::net::UnixListener,
    sync::{Arc, Mutex},
    time::Duration,
};
fn main() {
    gtk::init().unwrap();
    let scenario = std::env::args().nth(1).unwrap_or("current".into());
    let root = tempfile::tempdir().unwrap();
    let listener = UnixListener::bind(root.path().join("control.sock")).unwrap();
    let operations = Arc::new(Mutex::new(Vec::<String>::new()));
    let records = operations.clone();
    let phase = Arc::new(Mutex::new(scenario.clone()));
    let next_phase = phase.clone();
    std::thread::spawn(move || {
        for socket in listener.incoming() {
            let mut socket = socket.unwrap();
            let request: ipc::Request =
                serde_json::from_slice(&ipc::receive_frame(&mut socket).unwrap()).unwrap();
            records.lock().unwrap().push(request.op.clone());
            let result = match request.op.as_str() {
                "status" => {
                    json!({"unlocked":false,"error": if *phase.lock().unwrap() == "closure" { Some("Synthetic busy mount") } else { None }})
                }
                "update.status" => {
                    let phase = phase.lock().unwrap();
                    json!({
                        "phase": if *phase == "cooldown" { "error" } else { phase.as_str() },
                        "canCheck": *phase != "cooldown",
                        "progress": if *phase == "downloading" {
                            json!({"downloaded": 42, "total": 100})
                        } else {
                            serde_json::Value::Null
                        }
                    })
                }
                "launcher.continue" => {
                    assert_eq!(*phase.lock().unwrap(), "current");
                    serde_json::Value::Null
                }
                "update.check" => {
                    assert_eq!(*phase.lock().unwrap(), "error");
                    *phase.lock().unwrap() = "current".into();
                    serde_json::Value::Null
                }
                "launcher.recover" | "quit" | "update.install" => serde_json::Value::Null,
                other => panic!("Unexpected operation {other}"),
            };
            ipc::send_frame(
                &mut socket,
                &serde_json::to_vec(&json!({"result":result})).unwrap(),
            )
            .unwrap();
            if matches!(
                request.op.as_str(),
                "launcher.continue" | "launcher.recover" | "quit" | "update.install"
            ) {
                gtk::glib::MainContext::default().invoke(gtk::main_quit);
                break;
            }
        }
    });
    // Optional visual QA uses only the synthetic launcher window under Xvfb.
    if let Ok(path) = std::env::var("LAUNCHER_SCREENSHOT") {
        if std::env::var("LAUNCHER_MIN_SIZE").is_ok() {
            gtk::glib::timeout_add_local_once(Duration::from_millis(200), || {
                for widget in gtk::Window::list_toplevels() {
                    if let Some(window) = widget.downcast_ref::<gtk::Window>() {
                        window.resize(460, 380);
                    }
                }
            });
        }
        gtk::glib::timeout_add_local_once(Duration::from_millis(700), move || {
            use gtk::gdk::prelude::*;
            let widget = gtk::Window::list_toplevels().into_iter().next().unwrap();
            let window = widget.window().unwrap();
            window
                .pixbuf(0, 0, widget.allocated_width(), widget.allocated_height())
                .unwrap()
                .savev(&path, "png", &[])
                .unwrap();
        });
    }
    if matches!(
        scenario.as_str(),
        "error"
            | "cooldown"
            | "unsupported"
            | "closure"
            | "installing"
            | "checking"
            | "downloading"
    ) {
        let action = match scenario.as_str() {
            "closure" => "Retry vault closure",
            "error" => "Try again",
            _ => "Quit",
        };
        let installing = scenario == "installing";
        gtk::glib::timeout_add_local_once(Duration::from_millis(1000), move || {
            fn click(widget: &gtk::Widget, action: &str, installing: bool) -> bool {
                if let Some(button) = widget.downcast_ref::<gtk::Button>() {
                    assert_ne!(button.label().as_deref(), Some("Open current version"));
                    if button.label().as_deref() == Some(action) {
                        assert!(button.is_visible());
                        if installing {
                            assert!(!button.is_sensitive());
                        } else {
                            assert!(button.is_sensitive());
                            button.emit_clicked();
                        }
                        return true;
                    }
                }
                if let Some(container) = widget.downcast_ref::<gtk::Container>() {
                    return container.children().iter().fold(false, |found, child| {
                        click(child, action, installing) || found
                    });
                }
                false
            }
            assert!(gtk::Window::list_toplevels()
                .iter()
                .any(|window| click(window, action, installing)));
            if installing {
                *next_phase.lock().unwrap() = "current".into();
            }
        });
    }
    gtk::glib::timeout_add_local_once(Duration::from_secs(8), || panic!("Launcher did not finish"));
    launcher::run(root.path().into()).unwrap();
    let operations = operations.lock().unwrap();
    assert_eq!(
        operations.last().unwrap(),
        match scenario.as_str() {
            "ready" => "update.install",
            "closure" => "launcher.recover",
            "checking" | "downloading" | "cooldown" | "unsupported" => "quit",
            _ => "launcher.continue",
        }
    );
    assert!(!operations
        .iter()
        .any(|op| matches!(op.as_str(), "unlock" | "create")));
    println!("PASS: launcher {scenario}; no vault credentials or account initialization.");
}

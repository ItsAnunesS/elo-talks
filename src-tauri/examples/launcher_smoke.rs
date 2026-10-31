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
                "update.status" => json!({"phase":*phase.lock().unwrap(),"canCheck":false}),
                "launcher.continue" | "update.install" => serde_json::Value::Null,
                other => panic!("Unexpected operation {other}"),
            };
            ipc::send_frame(
                &mut socket,
                &serde_json::to_vec(&json!({"result":result})).unwrap(),
            )
            .unwrap();
            if matches!(request.op.as_str(), "launcher.continue" | "update.install") {
                gtk::glib::MainContext::default().invoke(gtk::main_quit);
                break;
            }
        }
    });
    if matches!(scenario.as_str(), "error" | "closure" | "installing") {
        let installing = scenario == "installing";
        gtk::glib::timeout_add_local_once(Duration::from_millis(1000), move || {
            fn click(widget: &gtk::Widget, installing: bool) {
                if let Some(button) = widget.downcast_ref::<gtk::Button>() {
                    if button.label().as_deref() == Some("Open current version") {
                        if installing {
                            assert!(!button.is_sensitive());
                        } else {
                            assert!(button.is_sensitive());
                            button.emit_clicked();
                        }
                    }
                }
                if let Some(container) = widget.downcast_ref::<gtk::Container>() {
                    for child in container.children() {
                        click(&child, installing);
                    }
                }
            }
            for window in gtk::Window::list_toplevels() {
                click(&window, installing);
            }
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
        if scenario == "ready" {
            "update.install"
        } else {
            "launcher.continue"
        }
    );
    assert!(!operations
        .iter()
        .any(|op| matches!(op.as_str(), "unlock" | "create")));
    println!("PASS: launcher {scenario}; no vault credentials or account initialization.");
}

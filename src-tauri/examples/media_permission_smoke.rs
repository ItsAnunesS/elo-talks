//! Offline getUserMedia with synthetic devices and the production consent dialog.
#[allow(dead_code)]
#[path = "../src/account_permissions.rs"]
mod account_permissions;
#[path = "support/cef_fixture.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "../src/storage.rs"]
mod storage;
#[allow(dead_code)]
#[path = "../src/whatsapp.rs"]
mod whatsapp;
use gtk::prelude::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use tauri::{webview::WebviewBuilder, LogicalSize, Manager, WebviewUrl};
use tauri_runtime_cef::{
    Cef, DebugEnvironment, DevToolsPolicy, RemoteDebugging, SandboxPolicy, WebviewBuilderCefExt,
    WebviewCefExt,
};
fn find_button(widget: &gtk::Widget, label: &str) -> Option<gtk::Button> {
    if let Some(button) = widget.downcast_ref::<gtk::Button>() {
        if button.label().as_deref() == Some(label) {
            return Some(button.clone());
        }
    }
    let mut child = widget.first_child();
    while let Some(widget) = child {
        if let Some(button) = find_button(&widget, label) {
            return Some(button);
        }
        child = widget.next_sibling();
    }
    None
}
fn answer(app: &tauri::AppHandle, allow: bool) {
    let label = if allow { "Allow" } else { "Don't allow" };
    fixture::wait_for(|| {
        let (tx, rx) = mpsc::sync_channel(1);
        app.run_on_main_thread(move || {
            for window in gtk::Window::list_toplevels() {
                if let Some(button) = find_button(&window, label) {
                    button.emit_clicked();
                    tx.send(true).unwrap();
                    return;
                }
            }
            tx.send(false).unwrap();
        })
        .unwrap();
        rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap()
    });
}
fn request(view: &tauri::Webview, video: bool) {
    fixture::evaluate(view, &format!("window.result='pending';navigator.mediaDevices.getUserMedia({{audio:{},video:{}}}).then(s=>{{window.stream=s;window.result='allowed'}}).catch(e=>window.result=e.name);true", !video, video));
}
fn reset(
    view: &tauri::Webview,
    permissions: &account_permissions::AccountPermissions,
    device: &str,
    block: bool,
) {
    let saved = permissions.reset(device, block).unwrap();
    view.with_cef_webview(move |native| {
        use tauri_runtime_cef::cef::{ImplBrowser, ImplFrame};
        let browser = native.browser();
        account_permissions::apply(&browser, &saved);
        browser
            .main_frame()
            .unwrap()
            .load_url(Some(&"https://web.whatsapp.com/".into()));
    })
    .unwrap();
    fixture::wait_for(|| fixture::evaluate(view, "window.result === undefined") == true);
}
#[tauri_runtime_cef::cef_entry_point]
fn main() {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        hook(panic);
        std::process::exit(1);
    }));
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("one")).unwrap();
    std::fs::create_dir(root.path().join("two")).unwrap();
    let one = account_permissions::AccountPermissions::load(root.path().join("one")).unwrap();
    let two = account_permissions::AccountPermissions::load(root.path().join("two")).unwrap();
    let passed = Arc::new(AtomicBool::new(false));
    let verified = passed.clone();
    tauri::Builder::default()
        .runtime(
            Cef::default()
                .root_cache_path(root.path())
                .allow_chromium_command_line_args(true) // Synthetic devices only, in this test executable.
                .sandbox(SandboxPolicy::Required)
                .debug_environment(DebugEnvironment::Deny)
                .remote_debugging(RemoteDebugging::Disabled)
                .devtools(DevToolsPolicy::Disallowed),
        )
        .setup(move |app| {
            let window = tauri::window::WindowBuilder::new(app, "media")
                .title("Media consent fixture")
                .inner_size(900., 640.)
                .build()?;
            account_permissions::set_parent(&window.gtk_window()?.upcast());
            let mut views = Vec::new();
            for (label, permissions) in [("one", one.clone()), ("two", two.clone())] {
                let handler = permissions.clone();
                let navigation = permissions.clone();
                let view = window.add_child(
                    WebviewBuilder::new(label, WebviewUrl::External("about:blank".parse()?))
                        .data_directory(root.path().join(label))
                        .size(LogicalSize::new(900., 640.))
                        .on_permission_request(|_, _| tauri::webview::PermissionResponse::Deny)
                        .on_media_permission(move |request| {
                            handler.request(request, "Synthetic account".into())
                        })
                        .on_frame_event(move |event| {
                            if event.is_main
                                && matches!(
                                    event.kind,
                                    tauri_runtime_cef::FrameEventKind::NavigationStarted { .. }
                                )
                            {
                                navigation.invalidate();
                            }
                        }),
                )?;
                views.push(view);
            }
            let app = app.handle().clone();
            std::thread::spawn(move || {
                let first = &views[0];
                let second = &views[1];
                for view in &views {
                    fixture::serve(
                        view,
                        "<!doctype html><title>Synthetic media</title><p>Offline media fixture</p>",
                    );
                    view.navigate("https://web.whatsapp.com/".parse().unwrap())
                        .unwrap();
                    fixture::wait_for(|| {
                        fixture::evaluate(view, "document.title") == "Synthetic media"
                    });
                }
                request(first, false);
                answer(&app, true);
                fixture::wait_for(|| fixture::evaluate(first, "window.result") == "allowed");
                assert_eq!(
                    one.snapshot().microphone,
                    account_permissions::Decision::Allow
                );
                assert_eq!(
                    two.snapshot().microphone,
                    account_permissions::Decision::Ask
                );
                request(first, true);
                answer(&app, false);
                fixture::wait_for(|| {
                    fixture::evaluate(first, "window.result") == "NotAllowedError"
                });
                assert_eq!(one.snapshot().camera, account_permissions::Decision::Block);
                reset(first, &one, "microphone", true);
                request(first, false);
                fixture::wait_for(|| {
                    fixture::evaluate(first, "window.result") == "NotAllowedError"
                });
                reset(first, &one, "camera", false);
                request(first, true);
                answer(&app, true);
                fixture::wait_for(|| fixture::evaluate(first, "window.result") == "allowed");
                // A prompt belonging to an old document cannot grant a new document.
                request(second, false);
                second
                    .navigate("https://example.invalid/".parse().unwrap())
                    .unwrap();
                fixture::wait_for(|| {
                    fixture::evaluate(second, "location.hostname") == "example.invalid"
                });
                answer(&app, true);
                assert_eq!(
                    two.snapshot().microphone,
                    account_permissions::Decision::Ask
                );
                request(second, false);
                fixture::wait_for(|| {
                    fixture::evaluate(second, "window.result") == "NotAllowedError"
                });
                passed.store(true, Ordering::SeqCst);
                app.exit(0);
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .unwrap();
    assert!(verified.load(Ordering::SeqCst));
    println!("PASS: native consent, separate devices/accounts, denial, revocation, re-prompt and stale/cross-origin rejection");
}

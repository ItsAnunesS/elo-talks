//! Synthetic CEF transport and worker notifications; no live accounts or IPC.
#[path = "../src/cef_notifications.rs"]
mod cef_notifications;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tauri::{webview::WebviewBuilder, WebviewUrl};
use tauri_runtime_cef::{Cef, DebugEnvironment, DevToolsPolicy, RemoteDebugging, SandboxPolicy};
#[tauri_runtime_cef::cef_entry_point]
fn main() {
    let root = tempfile::tempdir().unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let received = count.clone();
    let verified = count.clone();
    tauri::Builder::default()
        .runtime(Cef::default().sandbox(SandboxPolicy::Required)
            .root_cache_path(root.path()).debug_environment(DebugEnvironment::Deny)
            .remote_debugging(RemoteDebugging::Disabled).devtools(DevToolsPolicy::Disallowed)
            .allow_chromium_command_line_args(false)
            .default_content_setting(tauri_runtime_cef::cef::ContentSettingTypes::NOTIFICATIONS, tauri_runtime_cef::cef::ContentSettingValues::BLOCK))
        .register_uri_scheme_protocol("fixture", |_, request| {
            let (mime, body) = if request.uri().path() == "/worker.js" {
                ("text/javascript", "self.addEventListener('install', event => event.waitUntil(self.skipWaiting())); self.addEventListener('activate', event => event.waitUntil(self.clients.claim())); self.addEventListener('message', event => event.waitUntil(self.registration.showNotification('Synthetic sender', {body:'Synthetic private body'})));")
            } else {
                ("text/html", "<!doctype html><title>Synthetic CEF notifications</title><script>async function run(){new Notification('Synthetic sender',{body:'Synthetic private body'});const registration=await navigator.serviceWorker.register('/worker.js');await navigator.serviceWorker.ready; await registration.showNotification('Synthetic sender',{body:'Synthetic private body'});registration.active.postMessage('notify');} run().catch(error => {document.title='FIXTURE_FAILED:'+error.name+':'+error.message;});</script>")
            };
            tauri::http::Response::builder().header("Content-Type", mime).body(body.as_bytes().to_vec()).unwrap()
        })
        .setup(move |app| {
            let window = tauri::window::WindowBuilder::new(app,"test").title("Synthetic CEF notification transport").build()?;
            let view = window.add_child(WebviewBuilder::new("account-test", WebviewUrl::External("about:blank".parse()?)).size(tauri::LogicalSize::new(900.,640.)))?;
            cef_notifications::attach(&view, "http://fixture.localhost", move |title, body| {
                assert_eq!(title, "Synthetic sender");
                assert_eq!(body, "Synthetic private body");
                received.fetch_add(1, Ordering::SeqCst);
            }, || {}, || panic!("Synthetic notification transport failed"))?;

            let handle = app.handle().clone();
            std::thread::spawn(move || {
                for _ in 0..100 {
                    if count.load(Ordering::SeqCst) == 3 {
                        println!("PASS: CEF document, registration and service-worker notification transport");
                        handle.exit(0); return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                eprintln!("FAIL: CEF notification transport count={}", count.load(Ordering::SeqCst));
                handle.exit(1);
            });
            Ok(())
        }).run(tauri::generate_context!()).unwrap();
    assert_eq!(
        verified.load(Ordering::SeqCst),
        3,
        "All notification routes must be delivered"
    );
}

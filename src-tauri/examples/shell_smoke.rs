//! Production shell and account creation with synthetic IPC and offline profiles.
#[allow(dead_code)]
#[path = "../src/account_avatar.rs"]
mod account_avatar;
#[allow(dead_code)]
#[path = "../src/account_events.rs"]
mod account_events;
#[allow(dead_code)]
#[path = "../src/account_profiles.rs"]
mod account_profiles;
#[path = "../src/account_readiness.rs"]
mod account_readiness;
#[allow(dead_code)]
#[path = "../src/account_runtime.rs"]
mod account_runtime;
#[path = "../src/account_script.rs"]
mod account_script;
#[path = "../src/cef_notifications.rs"]
mod cef_notifications;
#[path = "../src/desktop_notifications.rs"]
mod desktop_notifications;
#[allow(dead_code)]
#[path = "../src/ipc.rs"]
mod ipc;
#[allow(dead_code)]
#[path = "../src/notification_content.rs"]
mod notification_content;
#[path = "../src/process_security.rs"]
mod process_security;
#[allow(dead_code)]
#[path = "../src/storage.rs"]
mod storage;
#[allow(dead_code)]
#[path = "../src/ui.rs"]
mod ui;
#[path = "../src/ui_layout.rs"]
mod ui_layout;
#[path = "../src/whatsapp.rs"]
mod whatsapp;
#[path = "../src/whatsapp_style.rs"]
mod whatsapp_style;
#[path = "../src/window_state.rs"]
mod window_state;
use gtk::prelude::*;
use serde_json::json;
use std::{
    os::unix::net::UnixListener,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tauri::{webview::WebviewBuilder, Manager, WebviewUrl};
#[path = "support/cef_fixture.rs"]
mod fixture;
#[tauri_runtime_cef::cef_entry_point]
fn main() {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        hook(panic);
        std::process::exit(1);
    }));
    unsafe {
        libc::umask(0o077);
        let limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        libc::setrlimit(libc::RLIMIT_CORE, &limit);
    }
    let root = tempfile::tempdir().unwrap();
    let listener = UnixListener::bind(root.path().join("control.sock")).unwrap();
    let accounts_enabled = Arc::new(AtomicBool::new(false));
    let server_accounts = accounts_enabled.clone();
    let status_seen = Arc::new(AtomicBool::new(false));
    let server_status = status_seen.clone();
    std::thread::spawn(move || {
        for socket in listener.incoming() {
            let mut socket = socket.unwrap();
            let request: ipc::Request =
                serde_json::from_slice(&ipc::receive_frame(&mut socket).unwrap()).unwrap();
            let result = match request.op.as_str() {
                "status" => {
                    server_status.store(true, Ordering::SeqCst);
                    let enabled = server_accounts.load(Ordering::SeqCst);
                    let accounts = if enabled {
                        json!([{"id":"11111111-1111-4111-8111-111111111111","name":"Synthetic one"},{"id":"22222222-2222-4222-8222-222222222222","name":"Synthetic two"}])
                    } else {
                        json!([])
                    };
                    json!({"created":enabled,"unlocked":enabled,"pin":false,"keys":[],"workspaces":[],"accounts":accounts,"categories":[],"idleMinutes":15,"version":"0.5.15","notifications":{"detail":"private","doNotDisturb":false,"mutedAccounts":[]}})
                }
                "update.status" | "update.startup" => json!({"phase":"current","canCheck":false}),
                "backup.status" => json!({"phase":"idle"}),
                "activity" => json!(null),
                _ => panic!("Unexpected synthetic controller operation: {}", request.op),
            };
            ipc::send_frame(
                &mut socket,
                &serde_json::to_vec(&json!({"result":result})).unwrap(),
            )
            .unwrap();
        }
    });
    let passed = Arc::new(AtomicBool::new(false));
    let verified = passed.clone();
    let external = std::env::var_os("ELO_TEST_PROFILE").map(std::path::PathBuf::from);
    let profile = external
        .clone()
        .unwrap_or_else(|| root.path().join("profile"));
    ui::run_with_account_setup(root.path().into(), profile, move |app| {
        assert_eq!(unsafe { libc::prctl(libc::PR_GET_DUMPABLE) }, 0);
        let handle = app.handle().clone();
        let shell = app.get_webview("main").unwrap();
        let remote = if external.is_none() {
            let view = app.get_window("main").unwrap().add_child(
                WebviewBuilder::new("account-untrusted-fixture", WebviewUrl::External("about:blank".parse().unwrap()))
                    .size(tauri::LogicalSize::new(100.,100.)))?;
            view.hide()?;
            Some(view)
        } else { None };
        std::thread::spawn(move || {
            for _ in 0..30 {
                std::thread::sleep(Duration::from_millis(300));
                let (sender, receiver) = std::sync::mpsc::sync_channel(1);
                shell.eval_with_callback("document.body.dataset.unlocked === 'false' && !document.getElementById('lock-screen').hidden", move |result| { let _ = sender.try_send(result); }).unwrap();
                if receiver.recv_timeout(Duration::from_secs(2)).ok().as_deref() == Some("true") {
                    passed.store(true,Ordering::SeqCst);
                    if let Some(profile) = &external {
                        let (sender,receiver) = std::sync::mpsc::sync_channel(1);
                        shell.eval_with_callback("localStorage.setItem('synthetic-persistence','preserve');true",move |value| { let _ = sender.send(value); }).unwrap();
                        assert_eq!(receiver.recv_timeout(Duration::from_secs(3)).unwrap(), "true");
                        std::fs::write(profile.join("cef-ready"), b"synthetic").unwrap();
                        return;
                    }
                    break;
                }
            }
            assert!(status_seen.load(Ordering::SeqCst), "The shell must call the native controller through IPC");
            if external.is_none() {
                for export in [true, false] {
                    eprintln!("Synthetic stage: backup chooser export={export}");
                    fixture::evaluate(&shell, &format!("window.backupChoice='pending';window.__TAURI__.core.invoke('choose_backup',{{export:{export}}}).then(value=>window.backupChoice=value).catch(()=>window.backupChoice='failed');true"));
                    let title = if export { "Export encrypted backup" } else { "Import encrypted backup" };
                    fixture::wait_for(|| {
                        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
                        handle.run_on_main_thread(move || {
                            let dialog = gtk::Window::list_toplevels().into_iter().find_map(|widget| widget.downcast::<gtk::Window>().ok().filter(|window| window.title().as_deref() == Some(title)));
                            let found = dialog.is_some();
                            if let Some(dialog) = dialog { dialog.close(); }
                            let _ = sender.send(found);
                        }).unwrap();
                        receiver.recv_timeout(Duration::from_secs(2)).unwrap()
                    });
                    fixture::wait_for(|| fixture::evaluate(&shell,"window.backupChoice === null") == true);
                }
            }
            if let Some(remote) = remote {
                eprintln!("Synthetic stage: remote native-command denial");
                fixture::serve(&remote, "<!doctype html><title>Synthetic remote origin</title>");
                remote.navigate("https://web.whatsapp.com/".parse().unwrap()).unwrap();
                fixture::wait_for(|| fixture::evaluate(&remote, "location.origin") == "https://web.whatsapp.com");
                fixture::evaluate(&remote, "window.denials=[];for(const command of ['request','account_snapshot','choose_backup','window_action'])window.__TAURI__.core.invoke(command,{op:'status',data:{},export:true,action:'close'}).then(()=>window.denials.push('unexpected success')).catch(error=>window.denials.push(String(error)));true");
                fixture::wait_for(|| fixture::evaluate(&remote, "window.denials.length === 4 && window.denials.every(error => error.includes('Unauthorized origin') || error.includes('not allowed'))") == true);
            }
            eprintln!("Synthetic stage: create accounts through production command");
            accounts_enabled.store(true, Ordering::SeqCst);
            fixture::evaluate(&shell, "window.accountsCreated=false;window.__TAURI__.core.invoke('sync_accounts').then(()=>window.accountsCreated=true).catch(()=>window.accountsCreated='failed');true");
            fixture::wait_for(|| fixture::evaluate(&shell,"window.accountsCreated") == true);
            assert!(handle.get_webview("account-11111111-1111-4111-8111-111111111111").is_some());
            assert!(handle.get_webview("account-22222222-2222-4222-8222-222222222222").is_some());
            eprintln!("Synthetic stage: account load/title callbacks keep the shell responsive");
            fixture::wait_for(|| handle.state::<account_events::AccountEvents>().runtime.lock().unwrap().accounts.values().all(|account| account.readiness == account_runtime::Readiness::Chats && account.unread_count == Some(1)));
            assert_eq!(fixture::evaluate(&shell,"document.readyState"), "complete");
            handle.exit(0);
        });
        Ok(())
    }, |view| {
        fixture::serve(view, "<!doctype html><title>WhatsApp</title><div id='pane-side' style='width:100px;height:100px'>Synthetic offline account</div><script>setTimeout(()=>document.title='(1) WhatsApp',500)</script>");
        Ok(())
    }).unwrap();
    assert!(
        verified.load(Ordering::SeqCst),
        "The real shell must initialize over CEF and IPC"
    );
    println!("PASS: production shell starts with synthetic controller, vault locked");
}

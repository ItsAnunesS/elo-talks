use std::{sync::mpsc, time::Duration};
use tauri::{WebviewUrl, WebviewWindowBuilder};
use tauri_runtime_cef::{
    AutoplayPolicy, Cef, DebugEnvironment, DevToolsPolicy, RemoteDebugging, SandboxPolicy,
    WebviewWindowBuilderCefExt,
};

// Synthetic-only executable: no vault path, IPC commands, or remote account access.
#[tauri_runtime_cef::cef_entry_point]
fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() == 3 && args[1] == "--live-profile" {
        live(std::path::PathBuf::from(&args[2]));
        return;
    }
    let notification_probe = args.len() == 2 && args[1] == "--notifications";
    assert!(
        args.len() == 1 || notification_probe,
        "Unsupported probe arguments"
    );
    let profile = tempfile::tempdir().expect("Cannot create synthetic profile");
    let (sender, receiver) = mpsc::sync_channel(1);
    let runtime = Cef::default()
        .sandbox(SandboxPolicy::Required)
        // Only generated audio is played by this synthetic, local-only fixture.
        .autoplay(AutoplayPolicy::NoUserGestureRequired)
        // Virtual devices make permission denial testable on a headless machine.
        .command_line_arg("use-fake-device-for-media-stream", None::<String>)
        .root_cache_path(profile.path())
        .debug_environment(DebugEnvironment::Deny)
        .remote_debugging(RemoteDebugging::Disabled)
        .devtools(DevToolsPolicy::Disallowed)
        .allow_chromium_command_line_args(false);
    tauri::Builder::default()
        .runtime(runtime)
        .setup(move |app| {
            WebviewWindowBuilder::new(
                app,
                "probe",
                WebviewUrl::App(if notification_probe {
                    "notifications.html".into()
                } else {
                    "index.html".into()
                }),
            )
            .title("Synthetic WebRTC probe — no WhatsApp accounts")
            .on_navigation(|url| {
                url.scheme() == "http" && url.host_str() == Some("tauri.localhost")
            })
            .on_permission_request(move |_, kind| {
                if notification_probe
                    && matches!(kind, tauri::webview::PermissionKind::Notifications)
                {
                    tauri::webview::PermissionResponse::Allow
                } else {
                    tauri::webview::PermissionResponse::Deny
                }
            })
            .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
            .on_console_message(move |event| {
                // Never forward arbitrary page output or URLs into diagnostics.
                let code = match event.message.as_str() {
                    "ELO_CEF_PASS" => 0,
                    "ELO_CEF_FAIL" => 1,
                    _ => return,
                };
                let _ = sender.try_send(code);
            })
            .build()?;
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let code = receiver.recv_timeout(Duration::from_secs(40)).unwrap_or(2);
                println!("Synthetic WebRTC result: {code} (0=pass, 1=fail, 2=timeout)");
                handle.exit(code);
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("CEF probe failed; sandbox is required");
}

// The launcher creates a separate, temporary gocryptfs vault and supervises this
// process. Never permit an account profile on an ordinary unencrypted directory.
fn live(profile: std::path::PathBuf) {
    let supervisor = std::env::var("ELO_CEF_SUPERVISOR_PID")
        .expect("Use live_test.py to supervise account access");
    let status = std::fs::read_to_string("/proc/self/status").expect("Process status unavailable");
    let parent = status
        .lines()
        .find_map(|line| line.strip_prefix("PPid:"))
        .unwrap_or("")
        .trim();
    assert!(
        parent != "1" && parent == supervisor,
        "Live account supervisor unavailable"
    );
    let profile = profile.canonicalize().expect("Live profile unavailable");
    let mounts = std::fs::read_to_string("/proc/self/mountinfo").expect("Mount status unavailable");
    let encrypted = mounts.lines().any(|line| {
        let Some((fields, _)) = line.split_once(" - fuse.gocryptfs ") else {
            return false;
        };
        let Some(mount) = fields.split_whitespace().nth(4) else {
            return false;
        };
        !mount.contains('\\')
            && profile.starts_with(mount)
            && profile != std::path::Path::new(mount)
    });
    assert!(
        encrypted,
        "Live accounts require a mounted gocryptfs test vault"
    );
    let runtime = Cef::default()
        .sandbox(SandboxPolicy::Required)
        .root_cache_path(&profile)
        .debug_environment(DebugEnvironment::Deny)
        .remote_debugging(RemoteDebugging::Disabled)
        .devtools(DevToolsPolicy::Disallowed)
        .allow_chromium_command_line_args(false);
    tauri::Builder::default()
        .runtime(runtime)
        .setup(|app| {
            WebviewWindowBuilder::new(
                app,
                "call-test",
                WebviewUrl::External("https://web.whatsapp.com/".parse()?),
            )
            .title("Elo CEF — temporary TEST account only")
            .on_navigation(|url| {
                url.scheme() == "https"
                    && url.host_str() == Some("web.whatsapp.com")
                    && url.port_or_known_default() == Some(443)
                    && url.username().is_empty()
                    && url.password().is_none()
            })
            .on_permission_request(|_, kind| match kind {
                // Chromium handles the actual requesting origin and user consent.
                tauri::webview::PermissionKind::Microphone
                | tauri::webview::PermissionKind::Camera => {
                    tauri::webview::PermissionResponse::Default
                }
                _ => tauri::webview::PermissionResponse::Deny,
            })
            .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
            .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Live CEF probe failed; sandbox is required");
}

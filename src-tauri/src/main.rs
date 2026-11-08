#![cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod account_avatar;
mod account_events;
mod account_profiles;
mod account_readiness;
mod account_runtime;
mod account_script;
mod auth;
mod backup;
mod cef_notifications;
mod controller;
mod crypto;
mod desktop_integration;
mod desktop_notifications;
mod ipc;
mod launcher;
mod notification_content;
mod process_security;
mod server_sync;
mod session_monitor;
mod storage;
mod ui;
mod ui_layout;
mod updater;
mod vault;
mod whatsapp;
mod whatsapp_style;
mod window_state;
mod workspaces;
use anyhow::{Context, Result};
use std::path::PathBuf;
#[tauri_runtime_cef::cef_entry_point]
fn main() {
    unsafe {
        libc::umask(0o077);
        let limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        libc::setrlimit(libc::RLIMIT_CORE, &limit);
    }
    if let Err(e) = (|| -> Result<()> {
        // CEF must bootstrap its user namespace before making the browser
        // nondumpable. Its default context is memory-only; ui::run protects
        // memory before creating the shell or opening any account context.
        if std::env::args_os().nth(1).is_none_or(|arg| arg != "--ui") {
            process_security::protect_secrets()?;
        }
        run()
    })() {
        eprintln!("Elo Talks: {e}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_some_and(|s| s == "--fido-worker") {
        return auth::hardware_worker();
    }
    if args.get(1).is_some_and(|s| s == "--launcher") {
        return launcher::run(PathBuf::from(
            args.get(2).context("Missing runtime directory")?,
        ));
    }
    if args.get(1).is_some_and(|s| s == "--ui") {
        return ui::run(
            PathBuf::from(args.get(2).context("Missing runtime directory")?),
            PathBuf::from(args.get(3).context("Missing profile")?),
        );
    }
    let home = PathBuf::from(std::env::var_os("HOME").context("HOME unavailable")?);
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".local/share"));
    let data = data_home.join("elo-talks");
    let runtime = PathBuf::from(
        std::env::var_os("XDG_RUNTIME_DIR").context("Linux session has no runtime directory")?,
    )
    .join("elo-talks");
    // Register the original AppImage path, not the transient mount path.
    if let Some(appimage) = std::env::var_os("APPIMAGE") {
        if desktop_integration::refresh(&data_home, std::path::Path::new(&appimage)).is_err() {
            // Desktop cosmetics must not prevent vault access; retry on the next launch.
            eprintln!("Elo Talks: desktop icon refresh was unavailable; it will be retried on next launch.");
        }
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));
        storage::private_dir(&config.join("autostart"))?;
        let exe = appimage.to_string_lossy();
        if !exe.contains(['\n', '\r']) {
            let quoted = exe
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('`', "\\`")
                .replace('$', "\\$")
                .replace('%', "%%");
            std::fs::write(config.join("autostart/dev.anuness.elotalks.desktop"), format!("[Desktop Entry]\nType=Application\nName=Elo Talks\nExec=\"{quoted}\"\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"))?;
            // Retire the previous app ID to avoid duplicate login launches.
            match std::fs::remove_file(config.join("autostart/com.elotalks.desktop.desktop")) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
    }
    controller::run(data, runtime)
}

#[cfg(test)]
mod cef_header_validation {
    use tauri::http;
    include!("../../vendor/tauri-runtime-cef/src/header_validation.rs");
}

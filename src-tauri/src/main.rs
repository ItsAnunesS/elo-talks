#![cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod account_events;
mod account_permissions;
mod account_runtime;
mod auth;
mod controller;
mod crypto;
mod desktop_integration;
mod desktop_notifications;
mod ipc;
mod notification_content;
mod session_monitor;
mod storage;
mod ui;
mod ui_layout;
mod vault;
mod workspaces;
use anyhow::{Context, Result};
use std::path::PathBuf;
fn main() {
    unsafe {
        libc::umask(0o077);
        let limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        libc::setrlimit(libc::RLIMIT_CORE, &limit);
        libc::prctl(libc::PR_SET_DUMPABLE, 0);
    }
    if let Err(e) = run() {
        eprintln!("Elo Talks: {e}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_some_and(|s| s == "--fido-worker") {
        return auth::hardware_worker();
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

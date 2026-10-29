pub static SESSION_SAFE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
use dbus::blocking::{stdintf::org_freedesktop_dbus::Properties, Connection};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
// The delay inhibitor gives the controller time to terminate sessions before suspend.
pub fn start(lock: Arc<AtomicBool>, shutdown: Arc<AtomicBool>) {
    std::thread::spawn(move || loop {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }
        if monitor(&lock, &shutdown).is_err() {
            SESSION_SAFE.store(false, Ordering::Relaxed);
            lock.store(true, Ordering::Relaxed);
            std::thread::sleep(Duration::from_secs(1));
        }
    });
}
fn monitor(lock: &AtomicBool, shutdown: &AtomicBool) -> Result<(), Box<dyn std::error::Error>> {
    let conn = Connection::new_system()?;
    let manager = conn.with_proxy(
        "org.freedesktop.login1",
        "/org/freedesktop/login1",
        Duration::from_secs(2),
    );
    let by_pid: Result<(dbus::Path<'static>,), dbus::Error> = manager.method_call(
        "org.freedesktop.login1.Manager",
        "GetSessionByPID",
        (std::process::id(),),
    );
    let session = match by_pid {
        Ok((path,)) => path,
        Err(_) => {
            // Desktop autostart may run in a user service outside the session scope.
            let (user,): (dbus::Path<'static>,) = manager.method_call(
                "org.freedesktop.login1.Manager",
                "GetUser",
                (unsafe { libc::getuid() },),
            )?;
            let user = conn.with_proxy("org.freedesktop.login1", user, Duration::from_secs(2));
            let (id, path): (String, dbus::Path<'static>) =
                user.get("org.freedesktop.login1.User", "Display")?;
            if id.is_empty() {
                return Err("No graphical login session".into());
            }
            path
        }
    };
    let session = conn.with_proxy("org.freedesktop.login1", session, Duration::from_secs(2));
    let mut inhibitor: Option<dbus::arg::OwnedFd> = None;
    while !shutdown.load(Ordering::Relaxed) {
        let sleep: bool = manager.get("org.freedesktop.login1.Manager", "PreparingForSleep")?;
        let locked: bool = session.get("org.freedesktop.login1.Session", "LockedHint")?;
        let state: String = session.get("org.freedesktop.login1.Session", "State")?;
        if state == "closing" {
            shutdown.store(true, Ordering::Relaxed);
        }
        SESSION_SAFE.store(!sleep && !locked && state != "closing", Ordering::Relaxed);
        if sleep || locked {
            lock.store(true, Ordering::Relaxed);
        }
        if sleep && inhibitor.is_some() {
            // Lock processing is bounded; dropping the inhibitor then lets logind suspend.
            std::thread::sleep(Duration::from_secs(3));
            inhibitor.take();
        } else if !sleep && inhibitor.is_none() {
            let (fd,): (dbus::arg::OwnedFd,) = manager.method_call(
                "org.freedesktop.login1.Manager",
                "Inhibit",
                (
                    "sleep",
                    "Elo Talks",
                    "Fechar o cofre antes de suspender",
                    "delay",
                ),
            )?;
            inhibitor = Some(fd);
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Ok(())
}

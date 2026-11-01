use crate::{
    account_runtime::NotificationPreferences,
    ipc::{receive_frame, send_frame, Request},
    storage,
    vault::Vault,
    workspaces::Catalog,
};
use anyhow::{anyhow, ensure, Context, Result};
use fs2::FileExt;
use serde_json::{json, Value};
use std::{
    fs::{self, OpenOptions},
    os::{
        fd::AsRawFd,
        unix::{fs::OpenOptionsExt, net::UnixListener, process::CommandExt},
    },
    path::PathBuf,
    process::{Child, Command},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

pub struct Controller {
    vault: Vault,
    child: Option<Child>,
    runtime: PathBuf,
    last_activity: Instant,
    retry_at: Instant,
    failures: u32,
    lock_error: Option<String>,
}
fn required_text<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .context("Required field missing")
}

impl Controller {
    fn spawn_ui(&mut self) -> Result<()> {
        let profile = if self.vault.master.is_some() && self.lock_error.is_none() {
            self.vault.mount.clone()
        } else {
            self.runtime.join("locked-ui")
        };
        storage::private_dir(&profile)?;
        let mut cmd = Command::new(std::env::current_exe()?);
        cmd.arg("--ui")
            .arg(&self.runtime)
            .arg(&profile)
            .process_group(0);
        cmd.env("WEBKIT_FORCE_SANDBOX", "1");
        cmd.env("XDG_CACHE_HOME", profile.join("cache"))
            .env("XDG_DATA_HOME", profile.join("data"));
        // No crash dumps, WebKit debugging, or user-provided sandbox bypasses.
        for name in [
            "WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS",
            "WEBKIT_INSPECTOR_SERVER",
            "GTK_MODULES",
            "GTK3_MODULES",
        ] {
            cmd.env_remove(name);
        }
        unsafe {
            cmd.pre_exec(|| {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
                Ok(())
            });
        }
        self.child = Some(cmd.spawn()?);
        Ok(())
    }
    fn spawn_launcher(&mut self) -> Result<()> {
        let mut command = Command::new(std::env::current_exe()?);
        command
            .arg("--launcher")
            .arg(&self.runtime)
            .process_group(0);
        for name in ["GTK_MODULES", "GTK3_MODULES", "GTK_PATH"] {
            command.env_remove(name);
        }
        unsafe {
            command.pre_exec(|| {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
                Ok(())
            });
        }
        self.child = Some(command.spawn()?);
        Ok(())
    }
    fn stop_ui(&mut self) -> Result<()> {
        if let Some(mut c) = self.child.take() {
            let pg = c.id() as i32;
            unsafe {
                libc::kill(-pg, libc::SIGTERM);
            }
            let start = Instant::now();
            while c.try_wait()?.is_none() && start.elapsed() < Duration::from_secs(1) {
                std::thread::sleep(Duration::from_millis(50));
            }
            unsafe {
                libc::kill(-pg, libc::SIGKILL);
            }
            let _ = c.wait();
            crate::account_events::clear_desktop_badge();
            // Unmount is authoritative: it must fail if another process still holds the mount.
        }
        Ok(())
    }
    fn lock(&mut self) -> Result<()> {
        self.stop_ui()?;
        match self.vault.unmount() {
            Ok(()) => self.lock_error = None,
            Err(e) => self.lock_error = Some(e.to_string()),
        }
        self.spawn_ui()?;
        Ok(())
    }
    fn accounts(&self) -> Result<Vec<crate::workspaces::Account>> {
        ensure!(
            self.vault.master.is_some() && self.lock_error.is_none(),
            "Vault locked"
        );
        Ok(Catalog::load(&self.vault.mount)?.accounts)
    }
    fn initialize_catalog(&mut self) -> Result<()> {
        if let Err(error) = Catalog::load(&self.vault.mount) {
            if let Err(close_error) = self.vault.unmount() {
                self.lock_error = Some(close_error.to_string());
                return Err(anyhow!("Could not initialize workspaces; vault closure requires attention: {close_error}"));
            }
            return Err(error.context("Could not initialize workspaces; the vault was closed"));
        }
        Ok(())
    }
    fn notification_preferences(&self) -> Result<NotificationPreferences> {
        if self.vault.master.is_none() || self.lock_error.is_some() {
            return Ok(NotificationPreferences::default());
        }
        let path = self.vault.mount.join("notifications.json");
        if path.exists() {
            let mut preferences: NotificationPreferences = storage::read(&path)?;
            let accounts = self.accounts()?;
            preferences
                .muted_accounts
                .retain(|id| accounts.iter().any(|account| &account.id == id));
            Ok(preferences)
        } else {
            Ok(NotificationPreferences::default())
        }
    }
    fn status(&self) -> Result<Value> {
        let manifest = self.vault.manifest.as_ref();
        let keys = manifest
            .map(|vault| {
                vault
                    .keys
                    .iter()
                    .map(|key| json!({"id": key.id, "name": key.name}))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let catalog = if self.vault.master.is_some() && self.lock_error.is_none() {
            Some(Catalog::load(&self.vault.mount)?)
        } else {
            None
        };
        Ok(json!({
            "created": manifest.is_some(),
            "unlocked": self.vault.master.is_some() && self.lock_error.is_none(),
            "pin": manifest.is_some_and(|vault| vault.pin.is_some()),
            "keys": keys,
            "accounts": catalog.as_ref().map(|c| &c.accounts).cloned().unwrap_or_default(),
            "organization": catalog.as_ref().map(Catalog::metadata),
            "lastAccount": catalog.as_ref().and_then(Catalog::preferred_account),
            "notifications": self.notification_preferences()?,
            "idleMinutes": manifest.map(|vault| vault.idle_minutes).unwrap_or(0),
            "error": self.lock_error,
            "version": env!("CARGO_PKG_VERSION"),
        }))
    }

    fn handle(&mut self, r: &Request) -> Result<Value> {
        let d = &r.data;
        match r.op.as_str() {
            "status" => return self.status(),
            "activity" => {
                self.last_activity = Instant::now();
                return Ok(Value::Null);
            }
            "create" | "unlock" => {
                ensure!(crate::session_monitor::SESSION_SAFE.load(Ordering::Relaxed), "Linux session locked or security monitor unavailable. Unlock the session and retry.");
                ensure!(
                    self.lock_error.is_none(),
                    "Resolve the unmount failure before continuing"
                );
                ensure!(
                    Instant::now() >= self.retry_at,
                    "Wait a few seconds before trying again"
                );
                let secret = Zeroizing::new(required_text(d, "secret")?.to_string());
                let result = if r.op == "create" {
                    self.vault.create(&secret)
                } else {
                    self.vault.unlock(
                        required_text(d, "method")?,
                        &secret,
                        d["keyId"].as_str().unwrap_or(""),
                    )
                };
                if result.is_err() {
                    self.failures = self.failures.saturating_add(1);
                    self.retry_at = Instant::now()
                        + Duration::from_secs((1u64 << self.failures.min(5)).min(30));
                }
                result?;
                // Authentication/mounting can outlive a screen lock or session switch.
                // Never initialize profiles when the monitor became unsafe meanwhile.
                if !crate::session_monitor::SESSION_SAFE.load(Ordering::Relaxed) {
                    if let Err(error) = self.vault.unmount() {
                        self.lock_error = Some(error.to_string());
                        return Err(error
                            .context("Session became unsafe; vault closure requires attention"));
                    }
                    return Err(anyhow!(
                        "Linux session became unsafe during unlock. Unlock the session and retry."
                    ));
                }
                self.initialize_catalog()?;
                self.failures = 0;
                self.last_activity = Instant::now();
            }
            "lock" | "quit" => {}
            _ => {
                ensure!(
                    self.vault.master.is_some() && self.lock_error.is_none(),
                    "Vault locked"
                );
                match r.op.as_str() {
                    "workspace.image" => {
                        return Catalog::load(&self.vault.mount)?.image(required_text(d, "id")?);
                    }
                    "account.add" | "account.rename" | "account.remove" | "account.reorder"
                    | "account.move" | "account.update" | "workspace.add" | "workspace.update"
                    | "workspace.remove" | "category.add" | "category.rename"
                    | "category.remove" => {
                        let mut catalog = Catalog::load(&self.vault.mount)?;
                        catalog.apply(&r.op, d)?;
                        catalog.save(&self.vault.mount)?;
                        // Profile deletion still happens only after every worker exits.
                    }
                    "account.activate" => {
                        let mut catalog = Catalog::load(&self.vault.mount)?;
                        let id = required_text(d, "id")?;
                        if catalog.activate(id)? {
                            catalog.save(&self.vault.mount)?;
                        }
                    }
                    "notifications" => {
                        let preferences: NotificationPreferences =
                            serde_json::from_value(d.clone())?;
                        let accounts = self.accounts()?;
                        ensure!(
                            preferences
                                .muted_accounts
                                .iter()
                                .all(|id| accounts.iter().any(|account| &account.id == id)),
                            "Unknown account in notification settings"
                        );
                        storage::write(&self.vault.mount.join("notifications.json"), &preferences)?;
                    }
                    "password" => self.vault.password(
                        required_text(d, "password")?,
                        required_text(d, "newPassword")?,
                    )?,
                    "pin" => self
                        .vault
                        .set_pin(required_text(d, "password")?, required_text(d, "pin")?)?,
                    "key.add" => self.vault.add_key(
                        required_text(d, "password")?,
                        required_text(d, "pin")?,
                        required_text(d, "name")?,
                    )?,
                    "key.remove" => {
                        self.vault.verify_password(required_text(d, "password")?)?;
                        self.vault
                            .manifest
                            .as_mut()
                            .unwrap()
                            .keys
                            .retain(|k| k.id != d["id"]);
                        self.vault.save()?;
                    }
                    "settings" => {
                        let minutes = d["idleMinutes"].as_u64().context("Invalid timeout")?;
                        ensure!(minutes <= 1440, "Invalid timeout");
                        self.vault.manifest.as_mut().unwrap().idle_minutes = minutes as u32;
                        self.vault.save()?;
                    }
                    _ => return Err(anyhow!("Operation not allowed")),
                }
            }
        }
        self.status()
    }
    fn prune_deleted_profiles(&self) -> Result<()> {
        let ids = self
            .accounts()?
            .into_iter()
            .map(|a| a.id)
            .collect::<Vec<_>>();
        let p = self.vault.mount.join("accounts");
        if !p.exists() {
            return Ok(());
        }
        for item in fs::read_dir(p)? {
            let item = item?;
            let id = item.file_name().to_string_lossy().into_owned();
            if uuid::Uuid::parse_str(&id).is_ok() && !ids.contains(&id) {
                fs::remove_dir_all(item.path())?;
            }
        }
        Ok(())
    }
}
pub fn run(base: PathBuf, runtime: PathBuf) -> Result<()> {
    storage::private_dir(&runtime)?;
    let guard = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(runtime.join("instance.lock"))?;
    if guard.try_lock_exclusive().is_err() {
        return Ok(());
    }
    let socket = runtime.join("control.sock");
    if socket.exists() {
        fs::remove_file(&socket)?;
    }
    let listener = UnixListener::bind(socket)?;
    listener.set_nonblocking(true)?;
    let mut updater = crate::updater::Updater::new(&runtime)?;
    storage::private_dir(&base)?;
    updater.configure(base.join("updates.json"))?;
    let mut startup_check = true;
    let mut installing = false;
    let vault = Vault::new(base, &runtime)?;
    let mut c = Controller {
        vault,
        child: None,
        runtime,
        last_activity: Instant::now(),
        retry_at: Instant::now(),
        failures: 0,
        lock_error: None,
    };
    // An interrupted previous run is never implicitly adopted.
    if c.vault.mounted() {
        if let Err(e) = c.vault.unmount() {
            c.lock_error = Some(e.to_string());
        }
    }
    let shutdown = Arc::new(AtomicBool::new(false));
    for sig in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
        signal_hook::flag::register(sig, shutdown.clone())?;
    }
    let lock = Arc::new(AtomicBool::new(false));
    crate::session_monitor::start(lock.clone(), shutdown.clone());
    c.spawn_launcher()?;
    loop {
        updater.poll();
        if installing && updater.status()["phase"] == "installed" {
            c.stop_ui()?;
            drop(listener);
            drop(guard);
            updater.restart()?;
            return Ok(());
        }
        if installing && updater.status()["phase"] == "error" {
            installing = false;
        }
        if shutdown.load(Ordering::Relaxed) {
            c.stop_ui()?;
            c.vault.unmount()?;
            break;
        }
        let timeout = c
            .vault
            .manifest
            .as_ref()
            .map(|m| m.idle_minutes)
            .unwrap_or(0);
        if c.vault.master.is_some()
            && c.lock_error.is_none()
            && (lock.swap(false, Ordering::Relaxed)
                || (timeout > 0 && c.last_activity.elapsed().as_secs() >= u64::from(timeout) * 60))
        {
            c.lock()?;
        }
        if c.child
            .as_mut()
            .map(|p| p.try_wait())
            .transpose()?
            .flatten()
            .is_some()
        {
            c.stop_ui()?;
            c.vault.unmount()?;
            return Err(anyhow!("The interface exited unexpectedly. The vault was closed. Check the error before reopening the application; it will not restart automatically."));
        }
        if let Ok((mut socket, _)) = listener.accept() {
            socket.set_read_timeout(Some(Duration::from_secs(2)))?;
            socket.set_write_timeout(Some(Duration::from_secs(2)))?;
            let mut peer: libc::ucred = unsafe { std::mem::zeroed() };
            let mut len = std::mem::size_of_val(&peer) as libc::socklen_t;
            let ok = unsafe {
                libc::getsockopt(
                    socket.as_raw_fd(),
                    libc::SOL_SOCKET,
                    libc::SO_PEERCRED,
                    (&mut peer as *mut libc::ucred).cast(),
                    &mut len,
                )
            };
            // Only the current trusted UI process may call the controller, not a remote WebKit renderer.
            if ok != 0
                || peer.uid != unsafe { libc::getuid() }
                || c.child.as_ref().map(|p| p.id() as i32) != Some(peer.pid)
            {
                continue;
            }
            let result = receive_frame(&mut socket)
                .and_then(|bytes| Ok(serde_json::from_slice::<Request>(&bytes)?));
            if let Ok(r) = result {
                if matches!(r.op.as_str(), "create" | "unlock")
                    && crate::session_monitor::SESSION_SAFE.load(Ordering::Relaxed)
                {
                    lock.store(false, Ordering::Relaxed);
                }
                let result = match r.op.as_str() {
                    "launcher.continue" => {
                        if installing
                            || c.lock_error.is_some()
                            || updater.status()["phase"] != "current"
                        {
                            Err(anyhow!(
                                "Complete the required update check before opening Elo Talks"
                            ))
                        } else {
                            startup_check = false;
                            Ok(Value::Null)
                        }
                    }
                    "launcher.recover" => {
                        if installing || c.lock_error.is_none() {
                            Err(anyhow!("Vault closure recovery is not available"))
                        } else {
                            Ok(Value::Null)
                        }
                    }
                    "update.startup" => {
                        let first = startup_check;
                        startup_check = false;
                        Ok(json!({"automatic": first}))
                    }
                    "update.status" => Ok(updater.status()),
                    "update.preferences" => Ok(updater.preferences()),
                    "update.configure" => {
                        if c.vault.master.is_none() || c.lock_error.is_some() {
                            Err(anyhow!("Unlock the vault to change settings"))
                        } else if let Some(minutes) = r.data["intervalMinutes"].as_u64() {
                            updater.set_interval(minutes)
                        } else {
                            Err(anyhow!("Invalid update interval"))
                        }
                    }
                    "update.check" => updater.command("check"),
                    "update.prepare" => updater.command("prepare"),
                    "update.install" => {
                        if updater.status()["phase"] == "ready" {
                            Ok(Value::Null)
                        } else {
                            Err(anyhow!("No verified update is ready"))
                        }
                    }
                    "create" | "unlock" | "quit" if installing => {
                        Err(anyhow!("Wait for the update to finish"))
                    }
                    _ => c.handle(&r),
                };
                let success = result.is_ok();
                let response = match result {
                    Ok(v) => json!({"result":v}),
                    Err(e) => json!({"error":e.to_string()}),
                };
                let _ = send_frame(&mut socket, &serde_json::to_vec(&response)?);
                if success {
                    match r.op.as_str() {
                        "launcher.continue" => {
                            c.stop_ui()?;
                            c.spawn_ui()?;
                        }
                        "launcher.recover" => {
                            c.stop_ui()?;
                            match c.vault.unmount() {
                                Ok(()) => c.lock_error = None,
                                Err(error) => c.lock_error = Some(error.to_string()),
                            }
                            c.spawn_launcher()?;
                        }
                        "update.install" => {
                            c.stop_ui()?;
                            match c.vault.unmount() {
                                Ok(()) => {
                                    if updater.command("install").is_ok() {
                                        installing = true;
                                        c.spawn_launcher()?;
                                    } else {
                                        updater.install_start_failed();
                                        c.spawn_launcher()?;
                                    }
                                }
                                Err(error) => {
                                    c.lock_error = Some(error.to_string());
                                    c.spawn_launcher()?;
                                }
                            }
                        }
                        "create" | "unlock" | "account.remove" => {
                            c.stop_ui()?;
                            c.prune_deleted_profiles()?;
                            c.spawn_ui()?;
                        }
                        "lock" => c.lock()?,
                        "quit" => {
                            c.stop_ui()?;
                            c.vault.unmount()?;
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspaces::Account;
    #[test]
    fn rejects_mutations_while_locked() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = tempfile::tempdir().unwrap();
        let vault = Vault::new(dir.path().into(), runtime.path()).unwrap();
        let mut c = Controller {
            vault,
            child: None,
            runtime: runtime.path().into(),
            last_activity: Instant::now(),
            retry_at: Instant::now(),
            failures: 0,
            lock_error: None,
        };
        for op in [
            "workspace.add",
            "workspace.image",
            "workspace.update",
            "workspace.remove",
            "category.rename",
            "category.remove",
            "account.update",
            "category.add",
            "account.move",
            "account.add",
            "account.activate",
            "account.remove",
            "settings",
            "notifications",
            "pin",
            "key.add",
            "password",
        ] {
            assert!(c
                .handle(&Request {
                    op: op.into(),
                    data: json!({})
                })
                .is_err());
        }
    }
    #[test]
    fn catalog_initialization_failure_drops_the_unlocked_key() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = tempfile::tempdir().unwrap();
        let mut vault = Vault::new(dir.path().into(), runtime.path()).unwrap();
        vault.master = Some(crate::crypto::random());
        storage::write(&vault.mount.join("catalog.json"), &json!({"version": 999})).unwrap();
        let mut controller = Controller {
            vault,
            child: None,
            runtime: runtime.path().into(),
            last_activity: Instant::now(),
            retry_at: Instant::now(),
            failures: 0,
            lock_error: None,
        };
        assert!(controller.initialize_catalog().is_err());
        assert!(controller.vault.master.is_none());
    }
    #[test]
    fn rejects_duplicate_account_order() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = tempfile::tempdir().unwrap();
        let mut vault = Vault::new(dir.path().into(), runtime.path()).unwrap();
        vault.master = Some(crate::crypto::random());
        let list = vec![
            Account {
                id: uuid::Uuid::new_v4().to_string(),
                name: "One".into(),
            },
            Account {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Two".into(),
            },
        ];
        storage::write(&vault.mount.join("accounts.json"), &list).unwrap();
        let mut c = Controller {
            vault,
            child: None,
            runtime: runtime.path().into(),
            last_activity: Instant::now(),
            retry_at: Instant::now(),
            failures: 0,
            lock_error: None,
        };
        assert!(c
            .handle(&Request {
                op: "account.reorder".into(),
                data: json!({"ids":[list[0].id,list[0].id]})
            })
            .is_err());
        assert_eq!(c.accounts().unwrap()[1].id, list[1].id);
    }
    #[test]
    fn notification_preferences_persist_and_reject_unknown_accounts() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = tempfile::tempdir().unwrap();
        let mut vault = Vault::new(directory.path().into(), runtime.path()).unwrap();
        vault.master = Some(crate::crypto::random());
        let account = Account {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Synthetic".into(),
        };
        storage::write(&vault.mount.join("accounts.json"), &vec![account.clone()]).unwrap();
        let mut controller = Controller {
            vault,
            child: None,
            runtime: runtime.path().into(),
            last_activity: Instant::now(),
            retry_at: Instant::now(),
            failures: 0,
            lock_error: None,
        };
        controller
            .handle(&Request {
                op: "notifications".into(),
                data: json!({"detail": "preview", "doNotDisturb": true, "mutedAccounts": [account.id]}),
            })
            .unwrap();
        let saved: NotificationPreferences =
            storage::read(&controller.vault.mount.join("notifications.json")).unwrap();
        assert!(saved.do_not_disturb);
        assert!(saved.muted_accounts.contains(&account.id));
        assert_eq!(
            saved.detail,
            crate::notification_content::NotificationDetail::Preview
        );
        assert!(controller
            .handle(&Request {
                op: "notifications".into(),
                data: json!({"detail": "invalid"}),
            })
            .is_err());
        assert!(controller
            .handle(&Request {
                op: "notifications".into(),
                data: json!({"mutedAccounts": ["unknown"]})
            })
            .is_err());
        assert!(
            controller
                .notification_preferences()
                .unwrap()
                .do_not_disturb
        );
        controller
            .handle(&Request {
                op: "account.remove".into(),
                data: json!({"id": account.id}),
            })
            .unwrap();
        let preferences = controller.notification_preferences().unwrap();
        assert!(preferences.muted_accounts.is_empty());
        assert!(preferences.do_not_disturb);
        assert_eq!(
            preferences.detail,
            crate::notification_content::NotificationDetail::Preview
        );
        controller
            .handle(&Request {
                op: "notifications".into(),
                data: serde_json::to_value(preferences).unwrap(),
            })
            .unwrap();
        controller.vault.master = None;
        assert_eq!(
            controller.notification_preferences().unwrap().detail,
            crate::notification_content::NotificationDetail::Private
        );
        assert!(
            !controller
                .notification_preferences()
                .unwrap()
                .do_not_disturb
        );
    }
}

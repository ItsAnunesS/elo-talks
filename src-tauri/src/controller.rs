use crate::{
    account_runtime::NotificationPreferences,
    backup,
    ipc::{receive_frame, send_frame, Request},
    server_sync, storage,
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
        mpsc, Arc,
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
    server: server_sync::Runtime,
    backup_job: Option<BackupJob>,
    backup_status: Value,
}
struct BackupJob {
    receiver: mpsc::Receiver<Result<Option<backup::Restore>>>,
    cancel: Arc<AtomicBool>,
}
impl Drop for BackupJob {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
fn required_text<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .context("Required field missing")
}

impl Controller {
    fn resume_after_server_change(&mut self) -> Result<()> {
        if !crate::session_monitor::SESSION_SAFE.load(Ordering::Relaxed) {
            self.lock()
        } else {
            self.spawn_ui()
        }
    }
    fn poll_server(&mut self) -> Result<()> {
        if self.vault.master.is_none()
            || self.lock_error.is_some()
            || !crate::session_monitor::SESSION_SAFE.load(Ordering::Relaxed)
        {
            self.server = server_sync::Runtime::default();
            return Ok(());
        }
        if let Some(result) = self.server.take_result() {
            match result {
                Ok(mut outcome) => {
                    let catalog = Catalog::load(&self.vault.mount)?;
                    let old_visible = self.server.verified;
                    let mut candidate = catalog.clone();
                    let new_selection = catalog.server.as_ref().is_none_or(|old| {
                        old.origin != outcome.connection.origin
                            || old.organization_id != outcome.connection.organization_id
                            || old.username != outcome.connection.username
                    });
                    if outcome.snapshot.is_some() || new_selection {
                        if let Err(error) = server_sync::apply_snapshot(
                            &mut candidate,
                            &mut outcome.connection,
                            outcome.snapshot.as_ref(),
                        ) {
                            self.server.status = json!({"phase":"error","error":error.to_string()});
                            self.server.next_check = Instant::now() + Duration::from_secs(30);
                            self.server.verified = false;
                            if old_visible {
                                self.stop_ui()?;
                                self.resume_after_server_change()?;
                            }
                            return Ok(());
                        }
                    } else {
                        candidate.server = Some(outcome.connection.clone());
                    }
                    let changed = serde_json::to_vec(&candidate.metadata())?
                        != serde_json::to_vec(&catalog.metadata())?;
                    if changed || !old_visible {
                        self.stop_ui()?;
                    }
                    let needs_save = *Zeroizing::new(serde_json::to_vec(&candidate)?)
                        != *Zeroizing::new(serde_json::to_vec(&catalog)?);
                    if let Err(error) = if needs_save {
                        candidate.save(&self.vault.mount)
                    } else {
                        Ok(())
                    } {
                        self.server.verified = false;
                        if catalog
                            .server
                            .as_ref()
                            .is_none_or(|old| old.token != outcome.connection.token)
                        {
                            server_sync::sign_out(outcome.connection.clone());
                        }
                        self.server.status = json!({"phase":"error","error":format!("Could not save synchronized workspaces: {error}")});
                        self.server.next_check = Instant::now() + Duration::from_secs(30);
                        if changed || !old_visible {
                            self.resume_after_server_change()?;
                        }
                        return Ok(());
                    }
                    if let Some(old) = &catalog.server {
                        if old.token != outcome.connection.token {
                            server_sync::sign_out(old.clone());
                        }
                    }
                    self.server.verified = true;
                    self.server.etag = outcome.etag;
                    self.server.status = json!({"phase":"connected","domain":outcome.connection.origin,"username":outcome.connection.username,"organizationId":outcome.connection.organization_id,"organizations":outcome.organizations});
                    self.server.next_check = Instant::now() + Duration::from_secs(30);
                    if changed || !old_visible {
                        self.prune_deleted_profiles()?;
                        self.resume_after_server_change()?;
                    }
                }
                Err(error) => {
                    let was_visible = self.server.verified;
                    self.server.verified = false;
                    self.server.etag = None;
                    let mut status = self.server.status.clone();
                    status["phase"] = json!("error");
                    status["error"] = json!(error.to_string());
                    self.server.status = status;
                    self.server.next_check = Instant::now() + Duration::from_secs(30);
                    if was_visible {
                        self.stop_ui()?;
                        self.resume_after_server_change()?;
                    }
                }
            }
        }
        if self.server.receiver.is_none() && Instant::now() >= self.server.next_check {
            let catalog = Catalog::load(&self.vault.mount)?;
            if let Some(connection) = catalog.server.clone() {
                if self.server.status["domain"].is_null() {
                    self.server.status = json!({"phase":"checking","domain":connection.origin,"username":connection.username,"organizationId":connection.organization_id,"organizations":[]});
                }
                self.server.start(
                    connection,
                    if self.server.verified {
                        self.server.etag.clone()
                    } else {
                        None
                    },
                )?;
            } else {
                self.server.next_check = Instant::now() + Duration::from_secs(30);
            }
        }
        Ok(())
    }
    fn server_command(&mut self, operation: &str, data: &Value) -> Result<Value> {
        ensure!(
            self.vault.master.is_some() && self.lock_error.is_none(),
            "Unlock the local vault first"
        );
        ensure!(
            crate::session_monitor::SESSION_SAFE.load(Ordering::Relaxed),
            "Unlock your Linux session before connecting"
        );
        if operation == "server.status" {
            return Ok(self.server.status.clone());
        }
        ensure!(
            self.server.receiver.is_none(),
            "Wait for the current server operation to finish"
        );
        let mut catalog = Catalog::load(&self.vault.mount)?;
        match operation {
            "server.connect" => self.server.connect(data, catalog.server.as_ref())?,
            "server.sync" | "server.select" => {
                let mut connection = catalog.server.clone().context("Connect a server first")?;
                if operation == "server.select" {
                    let id = required_text(data, "organizationId")?;
                    uuid::Uuid::parse_str(id)?;
                    connection.organization_id = Some(id.into());
                }
                self.server.start(connection, None)?;
            }
            "server.disconnect" => {
                ensure!(catalog.server.is_some(), "No server is connected");
                self.stop_ui()?;
                server_sync::remove_managed(&mut catalog);
                let previous_connection = catalog.server.take();
                if catalog.workspaces.is_empty() {
                    catalog.apply("workspace.add", &json!({"name":"My workspace"}))?;
                }
                let saved = catalog.save(&self.vault.mount);
                self.server = server_sync::Runtime::default();
                if saved.is_ok() {
                    if let Some(previous) = previous_connection {
                        server_sync::sign_out(previous);
                    }
                    self.prune_deleted_profiles()?;
                }
                self.resume_after_server_change()?;
                saved?;
            }
            _ => return Err(anyhow!("Unsupported server operation")),
        }
        Ok(self.server.status.clone())
    }
    fn start_backup(&mut self, operation: &str, data: &Value) -> Result<Value> {
        ensure!(
            self.backup_job.is_none(),
            "A backup operation is already running"
        );
        ensure!(
            self.lock_error.is_none() && self.vault.master.is_none() && !self.vault.mounted(),
            "Lock the vault before starting a backup operation"
        );
        ensure!(
            crate::session_monitor::SESSION_SAFE.load(Ordering::Relaxed),
            "Unlock your Linux session before starting a backup operation"
        );
        ensure!(
            Instant::now() >= self.retry_at,
            "Wait a few seconds before trying again"
        );
        let exporting = operation == "backup.export";
        if exporting {
            ensure!(
                self.vault.manifest.is_some(),
                "Create a vault before exporting"
            );
        } else {
            ensure!(
                self.vault.manifest.is_none()
                    && !self.vault.base.join("cipher").exists()
                    && !self.vault.base.join("restored-vault").exists(),
                "Import requires a new installation without an existing vault"
            );
        }
        let path = PathBuf::from(required_text(data, "path")?);
        ensure!(
            path.is_absolute() && path.as_os_str().len() <= 4096,
            "Choose an absolute backup path"
        );
        let password = Zeroizing::new(required_text(data, "password")?.to_owned());
        ensure!(password.len() <= 1024, "Password is too long");
        let backup_password =
            Zeroizing::new(data["backupPassword"].as_str().unwrap_or("").to_owned());
        if exporting {
            backup::validate_password(&backup_password)?;
        }
        let base = self.vault.base.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let (sender, receiver) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("vault-backup".into())
            .spawn(move || {
                let result = if exporting {
                    backup::export(&base, &path, &password, &backup_password, worker_cancel)
                        .map(|()| None)
                } else {
                    backup::prepare_restore(&base, &path, &password, worker_cancel).map(Some)
                };
                let _ = sender.send(result);
            })?;
        self.backup_job = Some(BackupJob { receiver, cancel });
        self.backup_status =
            json!({"phase": "running", "operation": if exporting { "export" } else { "import" }});
        Ok(self.backup_status.clone())
    }
    fn poll_backup(&mut self) -> Result<()> {
        let Some(job) = &self.backup_job else {
            return Ok(());
        };
        if !crate::session_monitor::SESSION_SAFE.load(Ordering::Relaxed) {
            job.cancel.store(true, Ordering::Relaxed);
        }
        let result = match job.receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return Ok(()),
            Err(mpsc::TryRecvError::Disconnected) => {
                Err(anyhow!("Backup worker stopped. Retry the operation."))
            }
        };
        let canceled = job.cancel.load(Ordering::Relaxed);
        self.backup_job = None;
        let result = result.and_then(|restore| {
            ensure!(
                !canceled,
                "Backup canceled because the Linux session became unsafe. Retry after unlocking."
            );
            if let Some(restore) = restore {
                let base = self.vault.base.clone();
                let installed = backup::install_restore(&base, &restore);
                // Reconcile even if directory fsync failed after atomic publication.
                self.vault = Vault::new(base, &self.runtime)?;
                installed?;
            }
            Ok(())
        });
        match result {
            Ok(()) => {
                self.failures = 0;
                self.backup_status["phase"] = json!("complete");
            }
            Err(error) => {
                self.failures = self.failures.saturating_add(1);
                self.retry_at =
                    Instant::now() + Duration::from_secs((1u64 << self.failures.min(5)).min(30));
                self.backup_status["phase"] = json!("error");
                self.backup_status["error"] = json!(error.to_string());
            }
        }
        Ok(())
    }
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
        cmd.env("XDG_CACHE_HOME", profile.join("cache"))
            .env("XDG_DATA_HOME", profile.join("data"))
            .env("XDG_CONFIG_HOME", profile.join("config"))
            .env("TMPDIR", crate::account_profiles::temporary(&profile)?);
        // No crash dumps, WebKit debugging, or user-provided sandbox bypasses.
        for name in [
            "WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS",
            "WEBKIT_INSPECTOR_SERVER",
            "GTK_MODULES",
            "GTK3_MODULES",
            "GTK_PATH",
            "CHROME_LOG_FILE",
            "CHROME_DEVEL_SANDBOX",
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
                libc::kill(pg, libc::SIGTERM);
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
        if self.vault.master.is_some() {
            crate::account_profiles::cleanup_process_links(&self.vault.mount)?;
        }
        crate::account_profiles::cleanup_process_links(&self.runtime.join("locked-ui"))?;
        Ok(())
    }
    fn lock(&mut self) -> Result<()> {
        self.server = server_sync::Runtime::default();
        let closure = self.stop_ui().and_then(|()| self.vault.unmount());
        match closure {
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
            let mut catalog = Catalog::load(&self.vault.mount)?;
            if !self.server.verified {
                server_sync::remove_managed(&mut catalog);
            }
            if catalog.workspaces.is_empty() {
                catalog.apply("workspace.add", &json!({"name":"My workspace"}))?;
            }
            Some(catalog)
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
            "backup": self.backup_status,
            "server": if self.vault.master.is_some() && self.lock_error.is_none() { self.server.status.clone() } else { json!({"phase":"locked"}) },
            "managedWorkspaces": catalog.as_ref().and_then(|catalog| catalog.server.as_ref()).map(|server| server.managed_workspaces.clone()).unwrap_or_default(),
            "version": env!("CARGO_PKG_VERSION"),
        }))
    }

    fn handle(&mut self, r: &Request) -> Result<Value> {
        let d = &r.data;
        match r.op.as_str() {
            "server.status" | "server.connect" | "server.sync" | "server.select"
            | "server.disconnect" => return self.server_command(&r.op, d),
            "backup.status" => return Ok(self.backup_status.clone()),
            "backup.export" | "backup.import" => return self.start_backup(&r.op, d),
            "status" => return self.status(),
            "activity" => {
                self.last_activity = Instant::now();
                return Ok(Value::Null);
            }
            "create" | "unlock" => {
                ensure!(
                    self.backup_job.is_none(),
                    "Wait for the backup operation to finish"
                );
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
                        ensure!(!server_sync::managed_mutation(&catalog, &r.op, d), "Manage this server workspace in the server panel. Local edits are not permitted.");
                        if r.op == "workspace.remove" {
                            if let Some(server) = &catalog.server {
                                ensure!(catalog.workspaces.iter().filter(|workspace| !server.managed_workspaces.contains(&workspace.id)).count() > 1,
                                    "Keep one local workspace available while connected to a server");
                            }
                        }
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
        for p in [
            self.vault.mount.join("accounts"),
            self.vault.mount.join("chromium/accounts"),
        ] {
            if !p.exists() {
                continue;
            }
            for item in fs::read_dir(p)? {
                let item = item?;
                let id = item.file_name().to_string_lossy().into_owned();
                if uuid::Uuid::parse_str(&id).is_ok() && !ids.contains(&id) {
                    fs::remove_dir_all(item.path())?;
                }
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
        server: server_sync::Runtime::default(),
        backup_job: None,
        backup_status: json!({"phase": "idle"}),
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
        c.poll_server()?;
        c.poll_backup()?;
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
        if let Some(exit_status) = c
            .child
            .as_mut()
            .map(|p| p.try_wait())
            .transpose()?
            .flatten()
        {
            c.stop_ui()?;
            c.vault.unmount()?;
            return Err(anyhow!("The interface exited unexpectedly ({exit_status}). The vault was closed. Check the error before reopening the application; it will not restart automatically."));
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
                        if c.backup_job.is_some() {
                            Err(anyhow!("Wait for the backup operation to finish"))
                        } else if updater.status()["phase"] == "ready" {
                            Ok(Value::Null)
                        } else {
                            Err(anyhow!("No verified update is ready"))
                        }
                    }
                    "create" | "unlock" | "quit" | "backup.export" | "backup.import"
                        if installing =>
                    {
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
    #[ignore = "requires isolated X11, host FUSE, ELO_TEST_HELPER and ELO_TEST_CEF_SHELL"]
    fn cef_shutdown_and_encrypted_backup() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = tempfile::tempdir().unwrap();
        let mut vault = Vault::new(directory.path().join("source"), runtime.path()).unwrap();
        vault.helper = PathBuf::from(std::env::var_os("ELO_TEST_HELPER").unwrap());
        vault.create("synthetic master password").unwrap();
        struct Cleanup(Controller);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = self.0.stop_ui();
                let _ = self.0.vault.unmount();
            }
        }
        let mut guard = Cleanup(Controller {
            vault,
            child: None,
            runtime: runtime.path().into(),
            last_activity: Instant::now(),
            retry_at: Instant::now(),
            failures: 0,
            lock_error: None,
            server: server_sync::Runtime::default(),
            backup_job: None,
            backup_status: json!({"phase":"idle"}),
        });
        let controller = &mut guard.0;
        let child = Command::new(std::env::var_os("ELO_TEST_CEF_SHELL").unwrap())
            .env("ELO_TEST_PROFILE", &controller.vault.mount)
            .env(
                "TMPDIR",
                crate::account_profiles::temporary(&controller.vault.mount).unwrap(),
            )
            .process_group(0)
            .spawn()
            .unwrap();
        let pid = child.id() as i32;
        controller.child = Some(child);
        let ready = controller.vault.mount.join("cef-ready");
        let start = Instant::now();
        while !ready.exists() && start.elapsed() < Duration::from_secs(15) {
            std::thread::sleep(Duration::from_millis(100));
        }
        let initialized = ready.exists();
        controller.stop_ui().unwrap();
        controller.vault.unmount().unwrap();
        assert!(
            initialized,
            "Production CEF shell must initialize inside the encrypted test vault"
        );
        assert!(!controller.vault.mounted());
        // Live members of the browser group must not survive closure; zombies
        // have no address space or open descriptors and are awaiting their reaper.
        for entry in fs::read_dir("/proc").unwrap().flatten() {
            let Ok(stat) = fs::read_to_string(entry.path().join("stat")) else {
                continue;
            };
            let Some((_, fields)) = stat.rsplit_once(") ") else {
                continue;
            };
            let fields: Vec<_> = fields.split_whitespace().collect();
            if fields.get(2).and_then(|value| value.parse::<i32>().ok()) == Some(pid) {
                assert_eq!(fields[0], "Z", "A browser process survived vault closure");
            }
        }
        crate::backup::export(
            &controller.vault.base,
            &directory.path().join("cef.elobackup"),
            "synthetic master password",
            "synthetic backup password",
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .unwrap();
    }

    #[test]
    fn backup_requires_closure_serializes_work_and_discards_canceled_restore() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = tempfile::tempdir().unwrap();
        let mut controller = Controller {
            vault: Vault::new(directory.path().into(), runtime.path()).unwrap(),
            child: None,
            runtime: runtime.path().into(),
            last_activity: Instant::now(),
            retry_at: Instant::now(),
            failures: 0,
            lock_error: None,
            server: server_sync::Runtime::default(),
            backup_job: None,
            backup_status: json!({"phase": "running"}),
        };
        controller.vault.master = Some(crate::crypto::random());
        for op in ["backup.export", "backup.import"] {
            assert!(controller
                .start_backup(op, &Value::Null)
                .unwrap_err()
                .to_string()
                .contains("Lock the vault"));
        }
        controller.vault.master = None;
        controller.lock_error = Some("Closure failed".into());
        assert!(controller
            .start_backup("backup.export", &Value::Null)
            .is_err());
        controller.lock_error = None;
        let (sender, receiver) = mpsc::sync_channel(1);
        let cancel = Arc::new(AtomicBool::new(true));
        controller.backup_job = Some(BackupJob { receiver, cancel });
        assert!(controller
            .start_backup("backup.import", &Value::Null)
            .unwrap_err()
            .to_string()
            .contains("already running"));
        for op in ["create", "unlock"] {
            assert!(controller
                .handle(&Request {
                    op: op.into(),
                    data: Value::Null
                })
                .unwrap_err()
                .to_string()
                .contains("Wait for the backup"));
        }
        let staged = directory.path().join(".backup-import-synthetic");
        storage::private_dir(&staged).unwrap();
        assert!(sender
            .send(Ok(Some(backup::Restore {
                path: staged.clone()
            })))
            .is_ok());
        controller.poll_backup().unwrap();
        assert_eq!(controller.backup_status["phase"], "error");
        assert!(!staged.exists());
        assert!(!directory.path().join("restored-vault").exists());
        assert!(controller.vault.manifest.is_none());
    }
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
            server: server_sync::Runtime::default(),
            backup_job: None,
            backup_status: json!({"phase": "idle"}),
        };
        for op in [
            "server.connect",
            "server.sync",
            "server.select",
            "server.disconnect",
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
            server: server_sync::Runtime::default(),
            backup_job: None,
            backup_status: json!({"phase": "idle"}),
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
            server: server_sync::Runtime::default(),
            backup_job: None,
            backup_status: json!({"phase": "idle"}),
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
            server: server_sync::Runtime::default(),
            backup_job: None,
            backup_status: json!({"phase": "idle"}),
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

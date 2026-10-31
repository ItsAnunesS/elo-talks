//! Public release checks run outside the vault controller's event loop.
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::{
    os::unix::process::CommandExt,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

const BACKGROUND_CHECK_INTERVAL: Duration = Duration::from_secs(5 * 60);

pub struct Updater {
    target: Option<PathBuf>,
    helper: PathBuf,
    child: Option<Child>,
    started: Instant,
    next_check: Instant,
    failures: u32,
    interval: Duration,
    preferences: Option<PathBuf>,
    last_check: Option<Instant>,
    state: Value,
    stage: Option<PathBuf>,
    output: PathBuf,
    progress: PathBuf,
}
impl Updater {
    pub fn new(runtime: &std::path::Path) -> Result<Self> {
        let target = std::env::var_os("APPIMAGE")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute() && p.is_file());
        let helper = std::env::current_exe()?
            .parent()
            .context("Executable directory unavailable")?
            .join("elo-updater");
        let expected = std::env::var("ELO_EXPECTED_UPDATE_VERSION").ok();
        std::env::remove_var("ELO_EXPECTED_UPDATE_VERSION");
        let mismatch = expected
            .as_deref()
            .is_some_and(|version| version != env!("CARGO_PKG_VERSION"));
        let state = if mismatch {
            json!({"phase":"error", "restartMismatch":true,
                "error":"The app restarted without loading the installed version. Close Elo Talks completely and reopen the installed AppImage."})
        } else {
            json!({"phase": if target.is_some() {"idle"} else {"unsupported"}})
        };
        Ok(Self {
            state,
            target,
            helper,
            child: None,
            started: Instant::now(),
            next_check: Instant::now() + BACKGROUND_CHECK_INTERVAL,
            failures: 0,
            interval: BACKGROUND_CHECK_INTERVAL,
            preferences: None,
            last_check: None,
            stage: None,
            output: runtime.join("update-result.json"),
            progress: runtime.join("update-progress.json"),
        })
    }
    pub fn configure(&mut self, path: PathBuf) -> Result<()> {
        if path.exists() {
            let value: Value = crate::storage::read(&path)?;
            let minutes = value["intervalMinutes"]
                .as_u64()
                .context("Invalid update interval")?;
            Self::validate_interval(minutes)?;
            self.interval = Duration::from_secs(minutes * 60);
            self.next_check = Instant::now() + self.interval;
        }
        self.preferences = Some(path);
        Ok(())
    }
    fn validate_interval(minutes: u64) -> Result<()> {
        ensure!(
            matches!(minutes, 5 | 10 | 15 | 30 | 60),
            "Choose 5, 10, 15, 30 or 60 minutes"
        );
        Ok(())
    }
    pub fn preferences(&self) -> Value {
        json!({"intervalMinutes": self.interval.as_secs() / 60})
    }
    pub fn set_interval(&mut self, minutes: u64) -> Result<Value> {
        Self::validate_interval(minutes)?;
        crate::storage::write(
            self.preferences
                .as_ref()
                .context("Update settings unavailable")?,
            &json!({"intervalMinutes": minutes}),
        )?;
        self.interval = Duration::from_secs(minutes * 60);
        // A settings change must never shorten an active error/server cooldown.
        let next = Instant::now() + self.interval;
        self.next_check = if self.failures > 0 {
            self.next_check.max(next)
        } else {
            next
        };
        Ok(self.preferences())
    }
    pub fn status(&self) -> Value {
        let now = Instant::now();
        let wait = if self.failures > 0 {
            self.next_check.saturating_duration_since(now)
        } else {
            self.last_check
                .map(|last| Duration::from_secs(60).saturating_sub(last.elapsed()))
                .unwrap_or_default()
        };
        let mut status = self.state.clone();
        status["currentVersion"] = json!(env!("CARGO_PKG_VERSION"));
        if self.state["phase"] == "downloading" {
            if let Ok(progress) = crate::storage::read::<Value>(&self.progress) {
                let total = progress["total"].as_u64().unwrap_or(0);
                let downloaded = progress["downloaded"].as_u64().unwrap_or(0);
                if matches!(
                    progress["step"].as_str(),
                    Some("downloading" | "verifying-release" | "verifying-download")
                ) && total <= 512 * 1024 * 1024
                    && downloaded <= total
                {
                    status["progress"] =
                        json!({"step":progress["step"], "downloaded":downloaded, "total":total});
                }
            }
        }
        status["canCheck"] = json!(
            self.target.is_some()
                && self.child.is_none()
                && wait.is_zero()
                && self.state["phase"] != "ready"
        );
        status["retryInSeconds"] = json!(wait.as_secs() + u64::from(wait.subsec_nanos() > 0));
        status["intervalMinutes"] = json!(self.interval.as_secs() / 60);
        status
    }
    pub fn install_start_failed(&mut self) {
        self.state = json!({"phase":"error", "error":"Could not start installation. The current version remains available."});
        self.schedule_next_check();
    }
    pub fn command(&mut self, operation: &str) -> Result<Value> {
        // Startup and the background scheduler may observe the same available release.
        // Reuse the in-flight/verified preparation rather than racing another helper.
        if operation == "prepare"
            && matches!(self.state["phase"].as_str(), Some("downloading" | "ready"))
        {
            return Ok(self.status());
        }
        ensure!(
            self.child.is_none(),
            "An update operation is already running"
        );
        ensure!(
            self.failures == 0 || Instant::now() >= self.next_check,
            "Update checks are paused after an error. Please try again later."
        );
        let target = self
            .target
            .as_ref()
            .context("Updates require an installed AppImage")?;
        let mut command = Command::new(&self.helper);
        command.arg(operation).arg(env!("CARGO_PKG_VERSION"));
        let phase = match operation {
            "check" => {
                ensure!(
                    self.state["phase"] != "ready",
                    "A verified update is already ready"
                );
                ensure!(
                    self.last_check
                        .is_none_or(|last| last.elapsed() >= Duration::from_secs(60)),
                    "Wait a minute before checking again"
                );
                "checking"
            }
            "prepare" => {
                ensure!(
                    self.state["phase"] == "available",
                    "Check for updates first"
                );
                command.arg(target).arg(
                    self.state["version"]
                        .as_str()
                        .context("Missing release version")?,
                );
                "downloading"
            }
            "install" => {
                ensure!(
                    self.state["phase"] == "ready",
                    "No verified update is ready"
                );
                command
                    .arg(target)
                    .arg(self.stage.as_ref().context("Missing staged update")?);
                "installing"
            }
            _ => anyhow::bail!("Unknown update operation"),
        };
        let _ = std::fs::remove_file(&self.progress);
        command.env("ELO_UPDATE_PROGRESS", &self.progress);
        let output = std::fs::File::create(&self.output)?;
        self.child = Some(
            command
                .stdin(Stdio::null())
                .stdout(output)
                .stderr(Stdio::null())
                .spawn()
                .context("Bundled updater unavailable")?,
        );
        self.started = Instant::now();
        if operation == "check" {
            self.last_check = Some(self.started);
        }
        self.state["phase"] = json!(phase);
        Ok(self.status())
    }
    pub fn poll(&mut self) {
        if let Some(child) = self.child.as_mut() {
            // Installation may include a fresh transparency verification; bound every helper.
            let limit = if self.state["phase"] == "checking" {
                15
            } else {
                300
            };
            if self.started.elapsed() > Duration::from_secs(limit) {
                let _ = child.kill();
                let _ = child.wait();
                self.child = None;
                self.state = json!({"phase":"error", "error":"Update timed out. You can continue using the app."});
            } else if let Ok(Some(status)) = child.try_wait() {
                self.child = None;
                let expected_version = self.state["version"].clone();
                self.state = std::fs::read(&self.output)
                    .ok()
                    .filter(|b| b.len() < 4096)
                    .and_then(|b| serde_json::from_slice(&b).ok())
                    .filter(|v: &Value| v["phase"].is_string())
                    .unwrap_or_else(|| json!({"phase":"error"}));
                if self.state["phase"] == "installed" && expected_version.is_string() {
                    self.state["version"] = expected_version;
                }
                if !status.success() {
                    let retry = self.state["retryAfter"].as_u64().unwrap_or(0);
                    self.state = json!({"phase":"error", "retryAfter":retry, "error":"The update could not be verified or installed. Try again later."});
                }
                self.stage = self
                    .state
                    .get("stage")
                    .and_then(Value::as_str)
                    .map(PathBuf::from)
                    .or(self.stage.take());
                self.state.as_object_mut().map(|o| o.remove("stage"));
                let _ = std::fs::remove_file(&self.output);
            }
            if self.child.is_none() {
                self.schedule_next_check();
            }
        } else if self.target.is_some() && self.state["phase"] == "available" {
            // Download and authenticate public artifacts while accounts remain open.
            // Only an explicit install request may close the vault and replace the app.
            if self.command("prepare").is_err() {
                self.state =
                    json!({"phase":"error", "error":"Could not start the background download."});
                self.schedule_next_check();
            }
        } else if self.target.is_some()
            && Instant::now() >= self.next_check
            && !matches!(self.state["phase"].as_str(), Some("ready" | "installed"))
            && self.command("check").is_err()
        {
            self.state = json!({"phase":"error"});
            self.schedule_next_check();
        }
    }
    fn schedule_next_check(&mut self) {
        let delay = if self.state["phase"] == "error" {
            self.failures = self.failures.saturating_add(1);
            let backoff = (self.interval.as_secs() * (1u64 << self.failures.min(4))).min(3600);
            Duration::from_secs(backoff.max(self.state["retryAfter"].as_u64().unwrap_or(0)))
        } else {
            self.failures = 0;
            self.interval
        };
        let now = Instant::now();
        // An unrepresentable server deadline must not result in immediate retries.
        self.next_check = now
            .checked_add(delay)
            .unwrap_or_else(|| now + Duration::from_secs(315_360_000));
    }
    pub fn restart(&self) -> Result<()> {
        let mut command = Command::new(self.target.as_ref().context("AppImage path unavailable")?);
        for key in [
            "APPIMAGE",
            "APPDIR",
            "ARGV0",
            "OWD",
            "LD_LIBRARY_PATH",
            "LD_PRELOAD",
            "GIO_MODULE_DIR",
            "GSETTINGS_SCHEMA_DIR",
            "GTK_PATH",
            "GST_PLUGIN_PATH",
            "GST_PLUGIN_SYSTEM_PATH",
            "GST_PLUGIN_SYSTEM_PATH_1_0",
            "GST_PLUGIN_PATH_1_0",
            "GST_PLUGIN_SCANNER",
            "GI_TYPELIB_PATH",
            "GDK_PIXBUF_MODULE_FILE",
            "GIO_EXTRA_MODULES",
            "WEBKIT_EXEC_PATH",
        ] {
            command.env_remove(key);
        }
        // AppRun prepends bundled search paths. Do not carry paths from the old
        // transient mount into the replacement process.
        if let Some(appdir) = std::env::var_os("APPDIR") {
            let appdir = PathBuf::from(appdir);
            for (key, value) in std::env::vars_os() {
                let paths: Vec<_> = std::env::split_paths(&value).collect();
                if paths.iter().any(|path| path.starts_with(&appdir)) {
                    let kept: Vec<_> = paths
                        .into_iter()
                        .filter(|path| !path.starts_with(&appdir))
                        .collect();
                    if kept.is_empty() {
                        command.env_remove(key);
                    } else if let Ok(value) = std::env::join_paths(kept) {
                        command.env(key, value);
                    }
                }
            }
        }
        command.env_remove("ELO_EXPECTED_UPDATE_VERSION");
        if self.state["phase"] == "installed" {
            if let Some(version) = self.state["version"].as_str() {
                command.env("ELO_EXPECTED_UPDATE_VERSION", version);
            }
        }
        // Replace the controller rather than leaving the new AppImage as a child
        // of the exiting old runtime. Success never returns to old application code.
        command.current_dir("/").stdin(Stdio::null());
        Err(command.exec()).context(
            "The update was installed, but restarting the AppImage failed. Reopen Elo Talks.",
        )
    }
}
impl Drop for Updater {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn failed_install_start_cannot_loop_back_into_ready() {
        let dir = tempfile::tempdir().unwrap();
        let mut updater = Updater::new(dir.path()).unwrap();
        updater.state = json!({"phase":"ready", "version":"0.4.1"});
        updater.install_start_failed();
        assert_eq!(updater.status()["phase"], "error");
        assert!(updater.command("install").is_err());
    }

    #[test]
    fn progress_exposes_only_bounded_public_sizes_during_download() {
        let dir = tempfile::tempdir().unwrap();
        let mut updater = Updater::new(dir.path()).unwrap();
        updater.state = json!({"phase":"downloading"});
        crate::storage::write(
            &updater.progress,
            &json!({"step":"downloading", "downloaded":25, "total":100, "url":"private"}),
        )
        .unwrap();
        assert_eq!(
            updater.status()["progress"],
            json!({"step":"downloading", "downloaded":25, "total":100})
        );
        crate::storage::write(
            &updater.progress,
            &json!({"step":"downloading", "downloaded":101, "total":100}),
        )
        .unwrap();
        assert!(updater.status().get("progress").is_none());
        updater.state = json!({"phase":"ready"});
        assert!(updater.status().get("progress").is_none());
    }

    #[test]
    fn interval_persists_and_cannot_bypass_cooldown() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("updates.json");
        let mut updater = Updater::new(dir.path()).unwrap();
        updater.configure(path.clone()).unwrap();
        assert!(updater.set_interval(1).is_err());
        updater.set_interval(15).unwrap();
        let mut restored = Updater::new(dir.path()).unwrap();
        restored.configure(path).unwrap();
        assert_eq!(restored.preferences()["intervalMinutes"], 15);
        restored.state = json!({"phase":"error", "retryAfter":7200});
        restored.schedule_next_check();
        let deadline = restored.next_check;
        restored.set_interval(5).unwrap();
        assert_eq!(restored.next_check, deadline);
        restored.state = json!({"phase":"current"});
        restored.schedule_next_check();
        assert_eq!(restored.failures, 0);
        assert!(
            restored
                .next_check
                .saturating_duration_since(Instant::now())
                <= Duration::from_secs(300)
        );
    }

    #[test]
    fn restart_replaces_process_and_removes_old_mount_environment() {
        const WORKER: &str = "ELO_RESTART_TEST_TARGET";
        if let Some(target) = std::env::var_os(WORKER) {
            let dir = tempfile::tempdir().unwrap();
            let mut updater = Updater::new(dir.path()).unwrap();
            updater.target = Some(PathBuf::from(target));
            updater.state = json!({"phase":"installed", "version":"0.2.999"});
            updater.restart().unwrap();
            panic!("Successful exec must never return");
        }
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("new.AppImage");
        let result = dir.path().join("result");
        std::fs::write(
            &target,
            r#"#!/bin/sh
[ -z "${APPIMAGE+x}" ] && [ -z "${APPDIR+x}" ] && [ -z "${GIO_MODULE_DIR+x}" ] || exit 41
[ "$PWD" = / ] || exit 42
[ "$ELO_EXPECTED_UPDATE_VERSION" = "0.2.999" ] || exit 43
printf '%s' "$$" > "$ELO_RESTART_TEST_RESULT"
"#,
        )
        .unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "updater::tests::restart_replaces_process_and_removes_old_mount_environment",
            ])
            .env(WORKER, &target)
            .env("ELO_RESTART_TEST_RESULT", &result)
            .env("APPIMAGE", "/tmp/old.AppImage")
            .env("APPDIR", "/tmp/old-mount")
            .env("GIO_MODULE_DIR", "/tmp/old-mount/modules")
            .spawn()
            .unwrap();
        let pid = child.id();
        assert!(child.wait().unwrap().success());
        assert_eq!(std::fs::read_to_string(result).unwrap(), pid.to_string());
    }

    #[test]
    fn install_retains_expected_version_for_restart_verification() {
        let dir = tempfile::tempdir().unwrap();
        let helper = dir.path().join("helper");
        std::fs::write(
            &helper,
            "#!/bin/sh\nprintf '%s' '{\"phase\":\"installed\"}'\n",
        )
        .unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut updater = Updater::new(dir.path()).unwrap();
        updater.target = Some(dir.path().join("image"));
        updater.helper = helper;
        updater.state = json!({"phase":"ready", "version":"0.2.999"});
        updater.stage = Some(dir.path().join("stage"));
        updater.command("install").unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while updater.child.is_some() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
            updater.poll();
        }
        assert_eq!(updater.status()["phase"], "installed");
        assert_eq!(updater.status()["version"], "0.2.999");
        assert!(updater.status().get("stage").is_none());
    }

    #[test]
    fn helper_failure_preserves_server_cooldown() {
        let dir = tempfile::tempdir().unwrap();
        let helper = dir.path().join("helper");
        std::fs::write(
            &helper,
            "#!/bin/sh\nprintf '%s' '{\"phase\":\"error\",\"retryAfter\":7200}'\nexit 1\n",
        )
        .unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut updater = Updater::new(dir.path()).unwrap();
        updater.target = Some(dir.path().join("image"));
        updater.helper = helper;
        updater.command("check").unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while updater.child.is_some() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
            updater.poll();
        }
        assert!(updater.child.is_none());
        assert_eq!(updater.status()["phase"], "error");
        assert_eq!(updater.status()["retryAfter"], 7200);
        assert!(
            updater.next_check.saturating_duration_since(Instant::now())
                > Duration::from_secs(7198)
        );
        assert!(updater.command("check").is_err());
    }

    #[test]
    fn retries_back_off_respect_server_deadlines_and_recover() {
        let dir = tempfile::tempdir().unwrap();
        let mut updater = Updater::new(dir.path()).unwrap();
        for seconds in [600, 1200, 2400, 3600, 3600] {
            updater.state = json!({"phase":"error"});
            updater.schedule_next_check();
            let remaining = updater.next_check.saturating_duration_since(Instant::now());
            assert!(remaining > Duration::from_secs(seconds - 2));
            assert!(remaining <= Duration::from_secs(seconds));
            assert!(updater.command("check").is_err());
        }
        updater.state = json!({"phase":"error", "retryAfter":7200});
        updater.schedule_next_check();
        assert!(
            updater.next_check.saturating_duration_since(Instant::now())
                > Duration::from_secs(7198)
        );
        updater.state = json!({"phase":"current"});
        updater.schedule_next_check();
        assert_eq!(updater.failures, 0);
        assert!(
            updater.next_check.saturating_duration_since(Instant::now())
                <= BACKGROUND_CHECK_INTERVAL
        );
    }

    #[test]
    fn background_check_never_installs_and_stage_is_not_exposed() {
        let dir = tempfile::tempdir().unwrap();
        let helper = dir.path().join("helper");
        std::fs::write(
            &helper,
            r#"#!/bin/sh
case "$1" in
check) printf '%s' '{"phase":"available","version":"0.1.99"}' ;;
prepare) printf '%s' '{"phase":"ready","version":"0.1.99","stage":"/synthetic/stage"}' ;;
*) exit 44 ;;
esac
"#,
        )
        .unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut updater = Updater::new(dir.path()).unwrap();
        updater.target = Some(dir.path().join("image"));
        updater.helper = helper;
        updater.next_check = Instant::now();
        updater.poll();
        assert_eq!(updater.status()["phase"], "checking");
        assert!(updater.command("prepare").is_err());
        let deadline = Instant::now() + Duration::from_secs(2);
        while updater.child.is_some() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
            updater.poll();
        }
        assert_eq!(updater.status()["phase"], "available");
        let remaining = updater.next_check.saturating_duration_since(Instant::now());
        assert!(remaining > Duration::from_secs(295));
        assert!(remaining <= Duration::from_secs(300));
        updater.poll();
        assert_eq!(updater.status()["phase"], "downloading");
        assert_eq!(updater.command("prepare").unwrap()["phase"], "downloading");
        let deadline = Instant::now() + Duration::from_secs(2);
        while updater.child.is_some() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
            updater.poll();
        }
        assert_eq!(updater.status()["phase"], "ready");
        updater.poll();
        assert!(
            updater.child.is_none(),
            "A prepared update must wait for explicit installation"
        );
        assert!(updater.command("check").is_err());
        assert_eq!(updater.status()["canCheck"], false);
        assert!(updater.stage.is_some());
        assert!(updater.status().get("stage").is_none());
    }
}

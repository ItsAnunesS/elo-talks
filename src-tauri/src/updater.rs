//! Public release checks run outside the vault controller's event loop.
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::{
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
    state: Value,
    stage: Option<PathBuf>,
    output: PathBuf,
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
        Ok(Self {
            state: json!({"phase": if target.is_some() {"idle"} else {"unsupported"}}),
            target,
            helper,
            child: None,
            started: Instant::now(),
            next_check: Instant::now() + BACKGROUND_CHECK_INTERVAL,
            failures: 0,
            stage: None,
            output: runtime.join("update-result.json"),
        })
    }
    pub fn status(&self) -> Value {
        self.state.clone()
    }
    pub fn command(&mut self, operation: &str) -> Result<Value> {
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
            "check" => "checking",
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
                self.state = std::fs::read(&self.output)
                    .ok()
                    .filter(|b| b.len() < 4096)
                    .and_then(|b| serde_json::from_slice(&b).ok())
                    .filter(|v: &Value| v["phase"].is_string())
                    .unwrap_or_else(|| json!({"phase":"error"}));
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
            let backoff = (300u64 * (1u64 << self.failures.min(4))).min(3600);
            Duration::from_secs(backoff.max(self.state["retryAfter"].as_u64().unwrap_or(0)))
        } else {
            self.failures = 0;
            BACKGROUND_CHECK_INTERVAL
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
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(())
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
            "#!/bin/sh\nprintf '%s' '{\"phase\":\"available\",\"version\":\"0.1.99\"}'\n",
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
        assert!(
            updater.child.is_none(),
            "Do not check again before the deadline"
        );
        assert_eq!(updater.status()["phase"], "available");
        assert!(updater.command("install").is_err());
        assert!(updater.stage.is_none());
        assert!(updater.status().get("stage").is_none());
    }
}

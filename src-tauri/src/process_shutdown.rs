//! Bounded graceful shutdown of a browser process group owned by this controller.
use anyhow::Result;
use std::{
    process::Child,
    time::{Duration, Instant},
};

fn signal(pid: i32, signal: i32) -> Result<()> {
    if unsafe { libc::kill(pid, signal) } != 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ESRCH) {
            return Err(error.into());
        }
    }
    Ok(())
}

/// Returns false when the deadline required a forced stop, never a successful flush.
pub fn stop(child: &mut Child, grace: Duration) -> Result<bool> {
    let group = child.id() as i32;
    if child.try_wait()?.is_none() {
        signal(group, libc::SIGTERM)?;
    }
    let deadline = Instant::now() + grace;
    let graceful = loop {
        if child.try_wait()?.is_some() {
            break true;
        }
        if Instant::now() >= deadline {
            break false;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    // Reap any remaining descendants before the caller attempts normal unmount.
    signal(-group, libc::SIGKILL)?;
    child.wait()?;
    Ok(graceful)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, os::unix::process::CommandExt, process::Command};

    fn start(script: &str, root: &std::path::Path) -> Child {
        let child = Command::new("/bin/sh")
            .args(["-c", script, "synthetic"])
            .arg(root)
            .process_group(0)
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !root.join("ready").exists() {
            assert!(Instant::now() < deadline, "Synthetic process did not start");
            std::thread::sleep(Duration::from_millis(10));
        }
        child
    }

    #[test]
    fn slow_session_write_finishes_before_process_teardown() {
        let root = tempfile::tempdir().unwrap();
        let mut child = start(
            "trap 'sleep 2; printf saved > \"$1/session\"; exit 0' TERM; touch \"$1/ready\"; while :; do sleep 0.05; done",
            root.path(),
        );
        assert!(stop(&mut child, Duration::from_secs(5)).unwrap());
        assert_eq!(fs::read(root.path().join("session")).unwrap(), b"saved");
        assert!(child.wait().unwrap().success());
    }

    #[test]
    fn unresponsive_process_is_stopped_without_claiming_graceful_closure() {
        let root = tempfile::tempdir().unwrap();
        let mut child = start(
            "trap '' TERM; touch \"$1/ready\"; while :; do sleep 1; done",
            root.path(),
        );
        assert!(!stop(&mut child, Duration::from_millis(100)).unwrap());
        assert!(!child.wait().unwrap().success());
    }
}

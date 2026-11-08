//! Apply memory protections before any credential or account context is opened.
use anyhow::{ensure, Result};
pub fn protect_secrets() -> Result<()> {
    ensure!(
        unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0) } == 0,
        "Cannot protect process memory"
    );
    ensure!(
        unsafe { libc::prctl(libc::PR_GET_DUMPABLE) } == 0,
        "Process memory protection unavailable"
    );
    Ok(())
}

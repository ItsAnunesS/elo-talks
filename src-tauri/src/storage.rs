use anyhow::{ensure, Result};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
};

pub fn private_dir(p: &Path) -> Result<()> {
    fs::create_dir_all(p)?;
    ensure!(
        !fs::symlink_metadata(p)?.file_type().is_symlink(),
        "Directory must not be a symbolic link"
    );
    fs::set_permissions(p, fs::Permissions::from_mode(0o700))?;
    Ok(())
}
const MAX_STATE_BYTES: u64 = 1024 * 1024;

pub fn read<T: DeserializeOwned>(p: &Path) -> Result<T> {
    let mut bytes = zeroize::Zeroizing::new(Vec::new());
    // O_NONBLOCK prevents a substituted FIFO from hanging the controller before
    // its file type can be checked. O_NOFOLLOW rejects the final symlink component.
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(p)?;
    ensure!(file.metadata()?.is_file(), "State must be a regular file");
    file.take(MAX_STATE_BYTES).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() < MAX_STATE_BYTES as usize,
        "File exceeds the size limit"
    );
    Ok(serde_json::from_slice(&bytes)?)
}
pub fn write<T: Serialize>(p: &Path, value: &T) -> Result<()> {
    let tmp = p.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)?;
    let result = (|| -> Result<()> {
        let bytes = zeroize::Zeroizing::new(serde_json::to_vec(value)?);
        f.write_all(&bytes)?;
        f.sync_all()?;
        fs::rename(&tmp, p)?;
        fs::File::open(p.parent().unwrap())?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(tmp);
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replaces_atomically_and_private() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("state.json");
        write(&p, &vec!["first"]).unwrap();
        write(&p, &vec!["second"]).unwrap();
        assert_eq!(read::<Vec<String>>(&p).unwrap(), ["second"]);
        assert_eq!(fs::metadata(p).unwrap().permissions().mode() & 0o777, 0o600);
    }
    #[test]
    fn rejects_links_directories_and_fifos_without_blocking() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target.json");
        write(&target, &vec!["private"]).unwrap();
        let link = dir.path().join("link.json");
        symlink(&target, &link).unwrap();
        assert!(read::<serde_json::Value>(&link).is_err());
        assert!(read::<serde_json::Value>(dir.path()).is_err());
        let fifo = dir.path().join("fifo");
        let name = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        assert_eq!(
            read::<serde_json::Value>(&fifo).unwrap_err().to_string(),
            "State must be a regular file"
        );
    }

    #[test]
    fn rejects_large_state_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("oversize.json");
        let file = fs::File::create(&path).unwrap();
        file.set_len(MAX_STATE_BYTES * 16).unwrap();
        let error = read::<serde_json::Value>(&path).unwrap_err();
        assert_eq!(error.to_string(), "File exceeds the size limit");
    }
}

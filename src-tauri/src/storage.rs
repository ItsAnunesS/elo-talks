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
    fs::File::open(p)?
        .take(MAX_STATE_BYTES)
        .read_to_end(&mut bytes)?;
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
        f.write_all(&serde_json::to_vec(value)?)?;
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
    fn rejects_large_state_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("oversize.json");
        let file = fs::File::create(&path).unwrap();
        file.set_len(MAX_STATE_BYTES * 16).unwrap();
        let error = read::<serde_json::Value>(&path).unwrap_err();
        assert_eq!(error.to_string(), "File exceeds the size limit");
    }
}

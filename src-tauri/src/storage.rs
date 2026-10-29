use anyhow::{ensure, Result};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
};

pub fn private_dir(p: &Path) -> Result<()> {
    fs::create_dir_all(p)?;
    ensure!(
        !fs::symlink_metadata(p)?.file_type().is_symlink(),
        "Diretório não pode ser um link"
    );
    fs::set_permissions(p, fs::Permissions::from_mode(0o700))?;
    Ok(())
}
pub fn read<T: DeserializeOwned>(p: &Path) -> Result<T> {
    let bytes = fs::read(p)?;
    ensure!(bytes.len() < 1024 * 1024, "Arquivo excede o limite");
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
}

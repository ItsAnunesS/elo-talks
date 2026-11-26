//! Chromium storage is additive: legacy WebKit profiles remain recoverable.
use anyhow::{Context, Result};
use std::{
    ffi::CString,
    fs,
    path::{Path, PathBuf},
};
pub fn root(vault: &Path) -> Result<PathBuf> {
    let root = vault.join("chromium");
    crate::storage::private_dir(&root)?;
    Ok(root)
}
pub fn account(vault: &Path, id: &str) -> Result<PathBuf> {
    uuid::Uuid::parse_str(id)?;
    let root = root(vault)?;
    // Chrome-backed CEF supports only direct children of root_cache_path.
    let profile = root.join(id);
    let previous_parent = root.join("accounts");
    match fs::symlink_metadata(&previous_parent) {
        Ok(_) => {
            crate::storage::private_dir(&previous_parent)?;
            let previous = previous_parent.join(id);
            match fs::symlink_metadata(&previous) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
                Ok(_) => {
                    crate::storage::private_dir(&previous)?;
                    let source = CString::new(previous.as_os_str().as_encoded_bytes())?;
                    let target = CString::new(profile.as_os_str().as_encoded_bytes())?;
                    // Never replace another profile, even if it appears during migration.
                    if unsafe {
                        libc::renameat2(
                            libc::AT_FDCWD,
                            source.as_ptr(),
                            libc::AT_FDCWD,
                            target.as_ptr(),
                            libc::RENAME_NOREPLACE,
                        )
                    } != 0
                    {
                        return Err(std::io::Error::last_os_error()).context("Account profile migration could not complete. Existing profiles were preserved; recovery is required.");
                    }
                    fs::File::open(&root)?.sync_all()?;
                    fs::File::open(&previous_parent)?.sync_all()?;
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    crate::storage::private_dir(&profile)?;
    Ok(profile)
}
/// Browser temporary data belongs to the same protected profile as its caches.
pub fn temporary(vault: &Path) -> Result<PathBuf> {
    let directory = vault.join(".cef-tmp");
    crate::storage::private_dir(&directory)?;
    Ok(directory)
}

/// Remove only Chromium's transient process rendezvous links after process shutdown.
/// These are not session data, and encrypted backups intentionally reject all links.
pub fn cleanup_process_links(vault: &Path) -> Result<()> {
    let temporary = vault.join(".cef-tmp");
    if temporary.try_exists()? {
        crate::storage::private_dir(&temporary)?;
        std::fs::remove_dir_all(temporary)?;
    }
    let directory = vault.join("chromium");
    if !directory.try_exists()? {
        return Ok(());
    }
    crate::storage::private_dir(&directory)?;
    for name in ["SingletonLock", "SingletonCookie", "SingletonSocket"] {
        let path = directory.join(name);
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) => {
                anyhow::ensure!(
                    metadata.file_type().is_symlink(),
                    "Unexpected Chromium process-lock entry"
                );
                std::fs::remove_file(path)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::fs::{symlink, PermissionsExt},
    };
    #[test]
    fn shutdown_cleanup_never_follows_process_links_or_deletes_sessions() {
        let vault = tempfile::tempdir().unwrap();
        let directory = root(vault.path()).unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        let scratch = temporary(vault.path()).unwrap();
        fs::write(
            scratch.join("transient"),
            b"synthetic temporary browser data",
        )
        .unwrap();
        fs::write(directory.join("session"), b"preserve").unwrap();
        for name in ["SingletonLock", "SingletonCookie", "SingletonSocket"] {
            symlink(outside.path(), directory.join(name)).unwrap();
        }
        cleanup_process_links(vault.path()).unwrap();
        assert!(!scratch.exists());
        assert!(outside.path().exists());
        assert_eq!(fs::read(directory.join("session")).unwrap(), b"preserve");
        fs::write(directory.join("SingletonLock"), b"unexpected").unwrap();
        assert!(cleanup_process_links(vault.path()).is_err());
        assert!(directory.join("SingletonLock").exists());
    }

    #[test]
    fn new_profiles_preserve_legacy_sessions_and_are_isolated() {
        let vault = tempfile::tempdir().unwrap();
        let first = uuid::Uuid::new_v4().to_string();
        let second = uuid::Uuid::new_v4().to_string();
        let legacy = vault.path().join("accounts").join(&first);
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join("session"), b"synthetic legacy session").unwrap();
        let chromium = account(vault.path(), &first).unwrap();
        fs::write(chromium.join("session"), b"synthetic chromium session").unwrap();
        let other = account(vault.path(), &second).unwrap();
        assert!(!other.join("session").exists());
        assert_eq!(
            fs::read(legacy.join("session")).unwrap(),
            b"synthetic legacy session"
        );
        assert_eq!(account(vault.path(), &first).unwrap(), chromium);
        assert_eq!(
            chromium.parent(),
            Some(root(vault.path()).unwrap().as_path())
        );
        assert_eq!(
            fs::metadata(chromium).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert!(account(vault.path(), "../outside").is_err());
    }
    #[test]
    fn nested_profiles_move_intact_and_conflicts_never_overwrite_data() {
        let vault = tempfile::tempdir().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let root = root(vault.path()).unwrap();
        let previous = root.join("accounts").join(&id);
        fs::create_dir_all(&previous).unwrap();
        fs::write(previous.join("session"), b"saved synthetic login").unwrap();
        let current = account(vault.path(), &id).unwrap();
        assert_eq!(current, root.join(&id));
        assert!(!previous.exists());
        assert_eq!(
            fs::read(current.join("session")).unwrap(),
            b"saved synthetic login"
        );
        assert_eq!(account(vault.path(), &id).unwrap(), current);
        fs::create_dir_all(&previous).unwrap();
        fs::write(previous.join("session"), b"other synthetic login").unwrap();
        assert!(account(vault.path(), &id).is_err());
        assert_eq!(
            fs::read(current.join("session")).unwrap(),
            b"saved synthetic login"
        );
        assert_eq!(
            fs::read(previous.join("session")).unwrap(),
            b"other synthetic login"
        );
    }

    #[test]
    fn profile_parents_cannot_redirect_storage_outside_the_vault() {
        let vault = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let root = root(vault.path()).unwrap();
        symlink(outside.path(), root.join("accounts")).unwrap();
        assert!(account(vault.path(), &uuid::Uuid::new_v4().to_string()).is_err());
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
    }
}

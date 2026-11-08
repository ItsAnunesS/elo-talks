//! Versioned, bounded snapshots of a closed vault. Staging contains ciphertext only.
use crate::{crypto, storage, vault::Manifest};
use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use anyhow::{anyhow, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use zeroize::Zeroizing;

const MAGIC: &[u8; 16] = b"ELO-BACKUP-0001\0";
const CONTEXT: &str = "elo-talks:backup:1";
const CHUNK: usize = 1024 * 1024;
const MAX_HEADER: usize = 16 * 1024;
const MAX_ENTRIES: usize = 200_000;
const MAX_BYTES: u64 = 64 * 1024 * 1024 * 1024;
const MAX_DEPTH: usize = 64;
const MAX_PATH_BYTES: usize = 32 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    path: String,
    // None is a directory; Some(0) is an empty file.
    size: Option<u64>,
}

pub fn validate_password(password: &str) -> Result<()> {
    ensure!(
        password.chars().count() >= 12 && password.len() <= 1024,
        "Use a backup password with at least 12 characters (maximum 1024 bytes)"
    );
    Ok(())
}
fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "Backup canceled. No existing vault was replaced."
    );
    Ok(())
}
fn regular(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    ensure!(
        file.metadata()?.is_file(),
        "Backup input must be a regular file"
    );
    Ok(file)
}

struct Frames<T> {
    io: T,
    cipher: Aes256Gcm,
    header: Vec<u8>,
    sequence: u64,
}
impl<T> Frames<T> {
    fn new(io: T, key: &[u8], header: Vec<u8>) -> Result<Self> {
        Ok(Self {
            io,
            cipher: Aes256Gcm::new_from_slice(key).map_err(|_| anyhow!("Invalid backup key"))?,
            header,
            sequence: 0,
        })
    }
    fn nonce(&mut self) -> Result<[u8; 12]> {
        let mut nonce = [0; 12];
        nonce[4..].copy_from_slice(&self.sequence.to_be_bytes());
        self.sequence = self.sequence.checked_add(1).context("Backup too large")?;
        Ok(nonce)
    }
}
impl<T: Write> Frames<T> {
    fn write(&mut self, bytes: &[u8]) -> Result<()> {
        ensure!(bytes.len() <= CHUNK, "Backup record too large");
        let nonce = self.nonce()?;
        let encrypted = self
            .cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: bytes,
                    aad: &self.header,
                },
            )
            .map_err(|_| anyhow!("Could not encrypt backup"))?;
        self.io.write_all(&(encrypted.len() as u32).to_be_bytes())?;
        self.io.write_all(&encrypted)?;
        Ok(())
    }
}
impl<T: Read> Frames<T> {
    fn read(&mut self) -> Result<Zeroizing<Vec<u8>>> {
        let mut length = [0; 4];
        self.io
            .read_exact(&mut length)
            .context("Backup is incomplete")?;
        let length = u32::from_be_bytes(length) as usize;
        ensure!(
            (16..=CHUNK + 16).contains(&length),
            "Invalid backup record size"
        );
        let mut bytes = vec![0; length];
        self.io
            .read_exact(&mut bytes)
            .context("Backup is incomplete")?;
        let nonce = self.nonce()?;
        Ok(Zeroizing::new(
            self.cipher
                .decrypt(
                    Nonce::from_slice(&nonce),
                    Payload {
                        msg: &bytes,
                        aad: &self.header,
                    },
                )
                .map_err(|_| anyhow!("Backup is damaged or has been modified"))?,
        ))
    }
}

fn collect(
    root: &Path,
    path: &Path,
    entries: &mut Vec<Entry>,
    total: &mut u64,
    path_bytes: &mut usize,
    cancel: &AtomicBool,
) -> Result<()> {
    check_cancel(cancel)?;
    ensure!(
        entries.len() < MAX_ENTRIES,
        "Backup contains too many files"
    );
    let relative = path.strip_prefix(root)?;
    ensure!(
        relative.components().count() <= MAX_DEPTH,
        "Backup directory nesting is too deep"
    );
    *path_bytes = path_bytes
        .checked_add(relative.as_os_str().len())
        .context("Backup metadata too large")?;
    ensure!(
        *path_bytes <= MAX_PATH_BYTES,
        "Backup path metadata exceeds the 32 MiB limit"
    );
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_dir() || metadata.is_file(),
        "Backup cannot include links or special files"
    );
    let size = if metadata.is_file() {
        *total = total
            .checked_add(metadata.len())
            .context("Backup too large")?;
        ensure!(*total <= MAX_BYTES, "Backup exceeds the 64 GiB limit");
        Some(metadata.len())
    } else {
        None
    };
    entries.push(Entry {
        path: relative
            .to_str()
            .context("Backup path is not UTF-8")?
            .into(),
        size,
    });
    if metadata.is_dir() {
        for item in fs::read_dir(path)? {
            collect(root, &item?.path(), entries, total, path_bytes, cancel)?;
        }
    }
    Ok(())
}

/// The controller must confirm normal unmount before calling this function.
pub fn export(
    base: &Path,
    destination: &Path,
    password: &str,
    backup_password: &str,
    cancel: Arc<AtomicBool>,
) -> Result<()> {
    check_cancel(&cancel)?;
    validate_password(backup_password)?;
    ensure!(password.len() <= 1024, "Invalid master password length");
    let canonical_base = fs::canonicalize(base)?;
    let parent = destination.parent().context("Choose a backup location")?;
    ensure!(
        !fs::canonicalize(parent)?.starts_with(&canonical_base),
        "Save the backup outside the Elo Talks data directory"
    );
    ensure!(
        !destination.exists(),
        "A file already exists at this location. Choose a new filename."
    );
    let mut manifest: Manifest = storage::read(&base.join("vault.json"))?;
    ensure!(manifest.schema == 1, "Unsupported vault version");
    let master = crypto::unwrap(
        &manifest.password,
        password.as_bytes(),
        &format!("{}:password", manifest.id),
    )?;
    manifest.password = crypto::wrap(
        &master,
        backup_password.as_bytes(),
        &format!("{}:password", manifest.id),
    )?;
    manifest.pin = None;
    manifest.keys.clear();
    let key = crypto::random();
    let header = serde_json::to_vec(&crypto::wrap(&key, backup_password.as_bytes(), CONTEXT)?)?;
    check_cancel(&cancel)?;
    let mut entries = vec![];
    collect(
        base,
        &base.join("cipher"),
        &mut entries,
        &mut 0,
        &mut 0,
        &cancel,
    )?;
    let temporary = parent.join(format!(".elo-backup-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(MAGIC)?;
        file.write_all(&(header.len() as u32).to_be_bytes())?;
        file.write_all(&header)?;
        let mut frames = Frames::new(file, &key, header)?;
        frames.write(&serde_json::to_vec(&manifest)?)?;
        let mut buffer = vec![0; CHUNK];
        for entry in entries {
            check_cancel(&cancel)?;
            frames.write(&serde_json::to_vec(&entry)?)?;
            if let Some(size) = entry.size {
                let mut input = regular(&base.join(&entry.path))?;
                ensure!(
                    input.metadata()?.len() == size,
                    "Vault changed during backup"
                );
                let mut remaining = size;
                while remaining > 0 {
                    check_cancel(&cancel)?;
                    let length = remaining.min(CHUNK as u64) as usize;
                    input.read_exact(&mut buffer[..length])?;
                    frames.write(&buffer[..length])?;
                    remaining -= length as u64;
                }
                ensure!(input.read(&mut [0])? == 0, "Vault changed during backup");
            }
        }
        frames.write(&[])?; // Authenticated end marker; truncation is never accepted.
        frames.io.sync_all()?;
        check_cancel(&cancel)?;
        // Atomic no-clobber publication on the destination filesystem.
        publish(&temporary, destination)?;
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    let _ = fs::remove_file(&temporary);
    result
}

fn publish(source: &Path, destination: &Path) -> Result<()> {
    let source_name = std::ffi::CString::new(source.as_os_str().as_encoded_bytes())?;
    let destination_name = std::ffi::CString::new(destination.as_os_str().as_encoded_bytes())?;
    let renamed = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            source_name.as_ptr(),
            libc::AT_FDCWD,
            destination_name.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if renamed == 0 {
        return Ok(());
    }
    let error = std::io::Error::last_os_error();
    if matches!(
        error.raw_os_error(),
        Some(libc::ENOSYS | libc::EINVAL | libc::EOPNOTSUPP)
    ) {
        fs::hard_link(source, destination).context("Could not save backup without overwriting a file. Choose a new filename on a local Linux filesystem.")?;
        return Ok(());
    }
    Err(error).context("Could not save backup. Choose a new filename in a writable folder.")
}

pub struct Restore {
    pub path: PathBuf,
}
impl Drop for Restore {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
fn safe_path(path: &str) -> Result<&Path> {
    let path = Path::new(path);
    ensure!(
        path.as_os_str().len() <= 4096 && path.components().count() <= MAX_DEPTH,
        "Invalid backup path"
    );
    ensure!(
        path.components()
            .all(|part| matches!(part, Component::Normal(_)))
            && path.components().next() == Some(Component::Normal("cipher".as_ref())),
        "Invalid backup path"
    );
    Ok(path)
}

pub fn prepare_restore(
    base: &Path,
    source: &Path,
    password: &str,
    cancel: Arc<AtomicBool>,
) -> Result<Restore> {
    check_cancel(&cancel)?;
    ensure!(password.len() <= 1024, "Invalid backup password length");
    let mut file = regular(source)?;
    ensure!(
        file.metadata()?.len() <= MAX_BYTES + MAX_BYTES / 8,
        "Backup exceeds the size limit"
    );
    let mut magic = [0; 16];
    file.read_exact(&mut magic)
        .context("Not an Elo Talks backup")?;
    ensure!(&magic == MAGIC, "Unsupported backup format or version");
    let mut length = [0; 4];
    file.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    ensure!(length <= MAX_HEADER, "Invalid backup header");
    let mut header = vec![0; length];
    file.read_exact(&mut header)?;
    let envelope = serde_json::from_slice(&header).context("Invalid backup header")?;
    let key = crypto::unwrap(&envelope, password.as_bytes(), CONTEXT)
        .map_err(|_| anyhow!("Incorrect backup password or damaged backup"))?;
    let mut frames = Frames::new(file, &key, header)?;
    let mut manifest: Manifest =
        serde_json::from_slice(&frames.read()?).context("Invalid backup manifest")?;
    ensure!(
        manifest.schema == 1
            && manifest.pin.is_none()
            && manifest.keys.is_empty()
            && manifest.idle_minutes <= 1440,
        "Unsupported backup manifest"
    );
    uuid::Uuid::parse_str(&manifest.id).context("Invalid backup vault identity")?;
    let master = crypto::unwrap(
        &manifest.password,
        password.as_bytes(),
        &format!("{}:password", manifest.id),
    )?;
    // A restored vault must not overwrite the source vault's keyring entry when
    // a new PIN is enrolled under the same Linux user.
    manifest.id = uuid::Uuid::new_v4().to_string();
    manifest.password = crypto::wrap(
        &master,
        password.as_bytes(),
        &format!("{}:password", manifest.id),
    )?;
    check_cancel(&cancel)?;
    let restore = Restore {
        path: base.join(format!(".backup-import-{}", uuid::Uuid::new_v4())),
    };
    storage::private_dir(&restore.path)?;
    let mut paths = HashSet::new();
    let mut total = 0u64;
    let mut path_bytes = 0usize;
    loop {
        check_cancel(&cancel)?;
        let record = frames.read()?;
        if record.is_empty() {
            break;
        }
        ensure!(
            record.len() <= 8192 && paths.len() < MAX_ENTRIES,
            "Backup contains too many files or an invalid entry"
        );
        let entry: Entry = serde_json::from_slice(&record).context("Invalid backup entry")?;
        let relative = safe_path(&entry.path)?;
        path_bytes = path_bytes
            .checked_add(entry.path.len())
            .context("Backup metadata too large")?;
        ensure!(
            path_bytes <= MAX_PATH_BYTES,
            "Backup path metadata exceeds the 32 MiB limit"
        );
        ensure!(paths.insert(relative.to_owned()), "Duplicate backup path");
        let output = restore.path.join(relative);
        if let Some(size) = entry.size {
            ensure!(relative != Path::new("cipher"), "Invalid cipher directory");
            total = total.checked_add(size).context("Backup too large")?;
            ensure!(total <= MAX_BYTES, "Backup exceeds the 64 GiB limit");
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(output)?;
            let mut remaining = size;
            while remaining > 0 {
                check_cancel(&cancel)?;
                let data = frames.read()?;
                ensure!(
                    data.len() == remaining.min(CHUNK as u64) as usize,
                    "Invalid backup file length"
                );
                file.write_all(&data)?;
                remaining -= data.len() as u64;
            }
            file.sync_all()?;
        } else {
            // Parents must already exist; links and implicit directories are never extracted.
            fs::create_dir(&output)?;
            storage::private_dir(&output)?;
        }
    }
    ensure!(
        frames.io.read(&mut [0])? == 0,
        "Unexpected data after backup end"
    );
    ensure!(
        restore.path.join("cipher/gocryptfs.conf").is_file()
            && restore.path.join("cipher/gocryptfs.diriv").is_file(),
        "Backup is missing vault configuration"
    );
    storage::write(&restore.path.join("vault.json"), &manifest)?;
    sync_directories(&restore.path)?;
    check_cancel(&cancel)?;
    Ok(restore)
}
fn sync_directories(path: &Path) -> Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            sync_directories(&entry.path())?;
        }
    }
    File::open(path)?.sync_all()?;
    Ok(())
}

/// The entire prepared vault becomes visible with one atomic directory rename.
pub fn install_restore(base: &Path, restore: &Restore) -> Result<()> {
    ensure!(
        !base.join("vault.json").exists()
            && !base.join("cipher").exists()
            && !base.join("restored-vault").exists(),
        "Import requires a new installation without an existing vault"
    );
    fs::rename(&restore.path, base.join("restored-vault"))?;
    File::open(base)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::Vault;
    use std::{io::Cursor, os::unix::fs::PermissionsExt};

    fn cancel() -> Arc<AtomicBool> {
        Arc::new(AtomicBool::new(false))
    }
    fn fixture(base: &Path) {
        storage::private_dir(&base.join("cipher/nested")).unwrap();
        fs::write(base.join("cipher/gocryptfs.conf"), b"synthetic config").unwrap();
        fs::write(base.join("cipher/gocryptfs.diriv"), [7; 16]).unwrap();
        fs::write(base.join("cipher/nested/session"), vec![42; CHUNK + 123]).unwrap();
        fs::write(base.join("cipher/empty"), []).unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let master = crypto::random();
        let envelope = crypto::wrap(
            &master,
            b"original master password",
            &format!("{id}:password"),
        )
        .unwrap();
        storage::write(
            &base.join("vault.json"),
            &Manifest {
                schema: 1,
                id,
                password: envelope.clone(),
                pin: Some(envelope),
                keys: vec![],
                idle_minutes: 12,
            },
        )
        .unwrap();
    }
    #[test]
    fn snapshot_round_trip_resets_pin_preserves_profiles_and_never_replaces_a_vault() {
        let source = tempfile::tempdir().unwrap();
        let destination = tempfile::tempdir().unwrap();
        let output = destination.path().join("test.elobackup");
        fixture(source.path());
        assert!(export(
            source.path(),
            &output,
            "wrong password",
            "different backup password",
            cancel()
        )
        .is_err());
        assert!(!output.exists());
        export(
            source.path(),
            &output,
            "original master password",
            "different backup password",
            cancel(),
        )
        .unwrap();
        assert_eq!(
            fs::metadata(&output).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let bytes = fs::read(&output).unwrap();
        assert!(!bytes
            .windows(b"nested/session".len())
            .any(|w| w == b"nested/session"));
        assert!(export(
            source.path(),
            &output,
            "original master password",
            "different backup password",
            cancel()
        )
        .is_err());
        let target = tempfile::tempdir().unwrap();
        assert!(prepare_restore(target.path(), &output, "wrong password", cancel()).is_err());
        assert_eq!(fs::read_dir(target.path()).unwrap().count(), 0);
        let restore = prepare_restore(
            target.path(),
            &output,
            "different backup password",
            cancel(),
        )
        .unwrap();
        assert!(install_restore(source.path(), &restore).is_err());
        install_restore(target.path(), &restore).unwrap();
        assert!(install_restore(target.path(), &restore).is_err());
        let runtime = tempfile::tempdir().unwrap();
        let vault = Vault::new(target.path().to_owned(), runtime.path()).unwrap();
        let manifest = vault.manifest.as_ref().unwrap();
        assert!(manifest.pin.is_none() && manifest.keys.is_empty());
        let original: Manifest = storage::read(&source.path().join("vault.json")).unwrap();
        assert_ne!(manifest.id, original.id);
        assert_eq!(manifest.idle_minutes, 12);
        vault.verify_password("different backup password").unwrap();
        assert!(vault.verify_password("original master password").is_err());
        assert_eq!(
            fs::read(vault.base.join("cipher/nested/session")).unwrap(),
            vec![42; CHUNK + 123]
        );
        assert!(fs::read(vault.base.join("cipher/empty"))
            .unwrap()
            .is_empty());
        // Truncation, authenticated modification and trailing bytes never publish.
        for modified in [
            bytes[..bytes.len() - 1].to_vec(),
            {
                let mut data = bytes.clone();
                let index = data.len() - 20;
                data[index] ^= 1;
                data
            },
            {
                let mut data = bytes.clone();
                data.push(0);
                data
            },
        ] {
            fs::write(&output, modified).unwrap();
            let empty = tempfile::tempdir().unwrap();
            assert!(
                prepare_restore(empty.path(), &output, "different backup password", cancel())
                    .is_err()
            );
            assert_eq!(fs::read_dir(empty.path()).unwrap().count(), 0);
        }
    }
    #[test]
    fn publication_never_clobbers_existing_files_and_early_cancel_writes_nothing() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source");
        let target = directory.path().join("target");
        fs::write(&source, b"new").unwrap();
        fs::write(&target, b"original").unwrap();
        assert!(publish(&source, &target).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"original");
        assert_eq!(fs::read(&source).unwrap(), b"new");
        let canceled = cancel();
        canceled.store(true, Ordering::Relaxed);
        assert!(export(
            directory.path(),
            &target,
            "password",
            "backup password",
            canceled.clone()
        )
        .is_err());
        assert!(prepare_restore(directory.path(), &source, "password", canceled).is_err());
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
    }
    #[test]
    #[ignore = "requires host FUSE and ELO_TEST_HELPER pointing to the pinned helper"]
    fn fuse_backup_restores_readable_catalog_and_profile_data() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = tempfile::tempdir().unwrap();
        let target_runtime = tempfile::tempdir().unwrap();
        let source = directory.path().join("source");
        let target = directory.path().join("target");
        let helper = PathBuf::from(std::env::var("ELO_TEST_HELPER").unwrap());
        let mut original = Vault::new(source.clone(), runtime.path()).unwrap();
        original.helper = helper.clone();
        original.create("original master password").unwrap();
        let mut saved_catalog = crate::workspaces::Catalog::load(&original.mount).unwrap();
        let workspace = saved_catalog.workspaces[0].id.clone();
        saved_catalog
            .apply(
                "account.add",
                &serde_json::json!({"name":"Synthetic migrated account", "workspaceId":workspace}),
            )
            .unwrap();
        saved_catalog.save(&original.mount).unwrap();
        let id = saved_catalog.accounts[0].id.clone();
        let legacy = original.mount.join("accounts").join(&id);
        storage::private_dir(&legacy).unwrap();
        fs::write(legacy.join("session"), b"legacy session retained").unwrap();
        let chromium = crate::account_profiles::account(&original.mount, &id).unwrap();
        fs::write(chromium.join("session"), b"new chromium session").unwrap();
        let catalog = saved_catalog.metadata();
        fs::write(
            original.mount.join("synthetic-session"),
            b"synthetic session marker",
        )
        .unwrap();
        original.unmount().unwrap();
        let output = directory.path().join("snapshot.elobackup");
        export(
            &source,
            &output,
            "original master password",
            "different backup password",
            cancel(),
        )
        .unwrap();
        storage::private_dir(&target).unwrap();
        let prepared =
            prepare_restore(&target, &output, "different backup password", cancel()).unwrap();
        install_restore(&target, &prepared).unwrap();
        let mut restored = Vault::new(target, target_runtime.path()).unwrap();
        restored.helper = helper;
        restored
            .unlock("password", "different backup password", "")
            .unwrap();
        let actual_catalog = crate::workspaces::Catalog::load(&restored.mount)
            .unwrap()
            .metadata();
        let actual_session = fs::read(restored.mount.join("synthetic-session")).unwrap();
        let legacy_session =
            fs::read(restored.mount.join("accounts").join(&id).join("session")).unwrap();
        let chromium_session = fs::read(
            restored
                .mount
                .join("chromium/accounts")
                .join(&id)
                .join("session"),
        )
        .unwrap();
        restored.unmount().unwrap();
        assert_eq!(legacy_session, b"legacy session retained");
        assert_eq!(chromium_session, b"new chromium session");
        assert_eq!(actual_catalog, catalog);
        assert_eq!(actual_session, b"synthetic session marker");
    }
    #[test]
    fn frames_reject_reordering_tampering_header_changes_and_oversized_allocations() {
        let key = crypto::random();
        let mut writer = Frames::new(Vec::new(), &key, b"header".to_vec()).unwrap();
        writer.write(b"first").unwrap();
        writer.write(b"second").unwrap();
        writer.write(&[]).unwrap();
        let bytes = writer.io;
        let mut reader = Frames::new(Cursor::new(&bytes), &key, b"header".to_vec()).unwrap();
        assert_eq!(&*reader.read().unwrap(), b"first");
        assert_eq!(&*reader.read().unwrap(), b"second");
        assert!(reader.read().unwrap().is_empty());
        assert!(reader.read().is_err());
        let mut changed = Frames::new(Cursor::new(&bytes), &key, b"other".to_vec()).unwrap();
        assert!(changed.read().is_err());
        let first_len = 4 + 5 + 16;
        let second_len = 4 + 6 + 16;
        let reordered = [
            &bytes[first_len..first_len + second_len],
            &bytes[..first_len],
        ]
        .concat();
        let mut reader = Frames::new(Cursor::new(reordered), &key, b"header".to_vec()).unwrap();
        assert!(reader.read().is_err());
        let mut reader = Frames::new(Cursor::new(u32::MAX.to_be_bytes()), &key, vec![]).unwrap();
        assert!(reader.read().is_err());
    }
    #[test]
    fn rejects_traversal_special_files_excessive_depth_and_cancellation() {
        for path in [
            "../outside",
            "/cipher/file",
            "cipher/../../outside",
            "cipher/../file",
            "other/file",
            "",
        ] {
            assert!(safe_path(path).is_err(), "{path}");
        }
        assert!(safe_path("cipher/nested/file").is_ok());
        assert!(safe_path(&format!("cipher/{}", "a/".repeat(MAX_DEPTH))).is_err());
        let source = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink("/etc/passwd", source.path().join("link")).unwrap();
        assert!(collect(
            source.path(),
            &source.path().join("link"),
            &mut vec![],
            &mut 0,
            &mut 0,
            &cancel()
        )
        .is_err());
        let canceled = cancel();
        canceled.store(true, Ordering::Relaxed);
        assert!(check_cancel(&canceled).is_err());
    }
}

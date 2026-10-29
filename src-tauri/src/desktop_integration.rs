//! Refresh existing per-user AppImage launchers without taking over their management.
use anyhow::{ensure, Context, Result};
use gtk::glib::{Checksum, ChecksumType, KeyFile, KeyFileFlags};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

const ICON: &[u8] = include_bytes!("../icons/icon.png");
const GROUP: &str = "Desktop Entry";
const MAX_ENTRY_BYTES: u64 = 64 * 1024;

pub fn refresh(data_home: &Path, appimage: &Path) -> Result<usize> {
    refresh_with_icon(data_home, appimage, ICON)
}

fn read_entry(path: &Path) -> Result<String> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.uid() == unsafe { libc::getuid() },
        "Launcher must be a user-owned regular file"
    );
    let mut text = String::new();
    file.take(MAX_ENTRY_BYTES + 1).read_to_string(&mut text)?;
    ensure!(text.len() <= MAX_ENTRY_BYTES as usize, "Launcher too large");
    Ok(text)
}

fn belongs_to_app(entry: &KeyFile, appimage: &Path) -> bool {
    let field = |key| entry.string(GROUP, key).ok();
    if field("Type").as_deref() != Some("Application") {
        return false;
    }
    if field("StartupWMClass").as_deref() != Some("elo-talks")
        && field("X-AppImage-Name").as_deref() != Some("Elo Talks")
    {
        return false;
    }
    // Match the actual installed AppImage, not just a name or substring of Exec.
    if let Some(executable) = field("TryExec") {
        return Path::new(executable.as_str()) == appimage;
    }
    let Some(command) = field("Exec") else {
        return false;
    };
    let Ok(arguments) = gtk::glib::shell_parse_argv(command.as_str()) else {
        return false;
    };
    let Some(first) = arguments.first() else {
        return false;
    };
    let executable = if first == "env" || first == "/usr/bin/env" {
        arguments
            .iter()
            .skip(1)
            .find(|argument| !argument.as_encoded_bytes().contains(&b'='))
    } else {
        Some(first)
    };
    executable.is_some_and(|path| Path::new(path) == appimage)
}

fn replace_file(path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.set_permissions(fs::Permissions::from_mode(mode))?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn icon_matches(path: &Path, expected: &[u8]) -> bool {
    let Ok(file) = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    else {
        return false;
    };
    if !file
        .metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.len() == expected.len() as u64)
    {
        return false;
    }
    let mut bytes = Vec::with_capacity(expected.len());
    file.take(expected.len() as u64 + 1)
        .read_to_end(&mut bytes)
        .is_ok()
        && bytes == expected
}

fn icon_path(data_home: &Path, bytes: &[u8]) -> Result<PathBuf> {
    let mut checksum = Checksum::new(ChecksumType::Sha256).context("SHA256 unavailable")?;
    checksum.update(bytes);
    let digest = checksum.string().context("Icon checksum unavailable")?;
    Ok(data_home
        .join("elo-talks/desktop-icons")
        .join(format!("elo-talks-{digest}.png")))
}

fn refresh_with_icon(data_home: &Path, appimage: &Path, icon: &[u8]) -> Result<usize> {
    ensure!(
        data_home.is_absolute() && appimage.is_absolute(),
        "Desktop paths must be absolute"
    );
    let applications = data_home.join("applications");
    if !applications.exists() {
        return Ok(0);
    }
    let metadata = fs::symlink_metadata(&applications)?;
    ensure!(
        metadata.is_dir() && metadata.uid() == unsafe { libc::getuid() },
        "Applications directory must be user-owned"
    );
    let target_icon = icon_path(data_home, icon)?;
    let target_text = target_icon.to_str().context("Icon path is not UTF-8")?;
    let mut changed = 0;
    for item in fs::read_dir(applications)? {
        let path = item?.path();
        if path
            .extension()
            .is_none_or(|extension| extension != "desktop")
        {
            continue;
        }
        // Ignore unrelated, malformed, oversized and symlinked desktop entries.
        let Ok(original) = read_entry(&path) else {
            continue;
        };
        let entry = KeyFile::new();
        if entry
            .load_from_data(
                &original,
                KeyFileFlags::KEEP_COMMENTS | KeyFileFlags::KEEP_TRANSLATIONS,
            )
            .is_err()
            || !belongs_to_app(&entry, appimage)
        {
            continue;
        }
        crate::storage::private_dir(target_icon.parent().unwrap())?;
        if fs::symlink_metadata(&target_icon)
            .is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            anyhow::bail!("Icon must not be a symbolic link");
        }
        if !icon_matches(&target_icon, icon) {
            replace_file(&target_icon, icon, 0o644)?;
        }
        if entry.string(GROUP, "Icon").ok().as_deref() == Some(target_text) {
            continue;
        }
        entry.set_string(GROUP, "Icon", target_text);
        // Avoid overwriting a concurrent manager edit. The next launch will retry.
        if read_entry(&path)? != original {
            continue;
        }
        let mode = fs::symlink_metadata(&path)?.permissions().mode() & 0o777;
        replace_file(&path, entry.to_data().as_bytes(), mode)?;
        changed += 1;
    }
    // Desktop environments watch .desktop files; replacing Icon invalidates the old path.
    // No shell restart, system-wide cache deletion or additional executable is required.
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn launcher(root: &Path, name: &str, target: &str) -> PathBuf {
        let applications = root.join("applications");
        fs::create_dir_all(&applications).unwrap();
        let path = applications.join(name);
        fs::write(&path, format!("# Manager comment\n[Desktop Entry]\nType=Application\nName=Custom label\nName[pt]=Personalizado\nStartupWMClass=elo-talks\nTryExec={target}\nExec=env DESKTOPINTEGRATION=1 \"{target}\" --custom\nIcon=old-icon\nX-Manager-Setting=keep\n\n[Desktop Action Open]\nName=Open\nIcon=action-icon\nExec=custom-command\n")).unwrap();
        path
    }

    #[test]
    fn refreshes_icon_without_rewriting_manager_fields_and_is_idempotent() {
        let root = tempfile::tempdir().unwrap();
        let appimage = Path::new("/home/example/My Apps/elo.appimage");
        let path = launcher(
            root.path(),
            "manager-name.desktop",
            appimage.to_str().unwrap(),
        );
        assert_eq!(
            refresh_with_icon(root.path(), appimage, b"first icon").unwrap(),
            1
        );
        let first = fs::read_to_string(&path).unwrap();
        assert!(first.contains("# Manager comment"));
        assert!(first.contains("Name[pt]=Personalizado"));
        assert!(first.contains("X-Manager-Setting=keep"));
        assert!(first.contains("Icon=action-icon"));
        assert!(first.contains(
            "Exec=env DESKTOPINTEGRATION=1 \"/home/example/My Apps/elo.appimage\" --custom"
        ));
        assert_eq!(
            refresh_with_icon(root.path(), appimage, b"first icon").unwrap(),
            0
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), first);
        assert_eq!(
            refresh_with_icon(root.path(), appimage, b"new icon").unwrap(),
            1
        );
        assert_ne!(fs::read_to_string(&path).unwrap(), first);
    }

    #[test]
    fn skips_other_installations_and_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let other = launcher(root.path(), "other.desktop", "/other/elo.appimage");
        let original = fs::read(&other).unwrap();
        symlink(&other, root.path().join("applications/link.desktop")).unwrap();
        assert_eq!(
            refresh_with_icon(root.path(), Path::new("/current/elo.appimage"), b"icon").unwrap(),
            0
        );
        assert_eq!(fs::read(&other).unwrap(), original);
        assert!(!root.path().join("elo-talks").exists());
    }

    #[test]
    fn supports_quoted_exec_without_tryexec() {
        let entry = KeyFile::new();
        entry.load_from_data("[Desktop Entry]\nType=Application\nX-AppImage-Name=Elo Talks\nExec=env DESKTOPINTEGRATION=1 \"/home/example/My Apps/elo.appimage\" %U\n", KeyFileFlags::NONE).unwrap();
        assert!(belongs_to_app(
            &entry,
            Path::new("/home/example/My Apps/elo.appimage")
        ));
        assert!(!belongs_to_app(&entry, Path::new("/home/example/My")));
    }
    #[test]
    fn repairs_a_launcher_regenerated_by_an_update_manager() {
        let root = tempfile::tempdir().unwrap();
        let path = launcher(root.path(), "elo_talks.desktop", "/apps/elo.appimage");
        let manager_original = fs::read(&path).unwrap();
        let appimage = Path::new("/apps/elo.appimage");
        refresh_with_icon(root.path(), appimage, b"logo").unwrap();
        let repaired = fs::read(&path).unwrap();
        fs::write(&path, manager_original).unwrap();
        assert_eq!(
            refresh_with_icon(root.path(), appimage, b"logo").unwrap(),
            1
        );
        assert_eq!(fs::read(&path).unwrap(), repaired);
    }

    #[test]
    fn does_not_follow_a_matching_launcher_symlink() {
        let root = tempfile::tempdir().unwrap();
        let path = launcher(root.path(), "original.desktop", "/apps/elo.appimage");
        let external = root.path().join("external-launcher");
        fs::rename(&path, &external).unwrap();
        let original = fs::read(&external).unwrap();
        symlink(&external, &path).unwrap();
        assert_eq!(
            refresh_with_icon(root.path(), Path::new("/apps/elo.appimage"), b"logo").unwrap(),
            0
        );
        assert_eq!(fs::read(&external).unwrap(), original);
    }
}

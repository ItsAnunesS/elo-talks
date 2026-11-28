//! User-initiated file transfers. The page cannot request clipboard or filesystem reads.
use anyhow::{ensure, Context, Result};
use gtk::{glib, prelude::*};
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::Duration,
};
use tauri::{Emitter, Manager};
use tauri_runtime_cef::{
    cef::{CefString, ImplBrowser, ImplFrame},
    WebviewCefExt,
};

const MAX_FILES: usize = 100;
const MAX_URI_BYTES: usize = 64 * 1024;
const MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const CAPTURE: &str = include_str!("attachment_capture.js");
const DISPATCH: &str = include_str!("attachment_dispatch.js");
const CLEANUP: &str = "function () { this.eloCancelled = true; this.remove(); }";
type Eligible = dyn Fn() -> bool + Send + Sync;

pub struct Attachments {
    view: tauri::Webview,
    script: Arc<crate::account_script::Script>,
    forbidden: PathBuf,
    eligible: Arc<Eligible>,
    busy: AtomicBool,
}
impl Attachments {
    pub fn attach(
        view: &tauri::Webview,
        forbidden: PathBuf,
        eligible: impl Fn() -> bool + Send + Sync + 'static,
    ) -> tauri::Result<Arc<Self>> {
        Ok(Arc::new(Self {
            view: view.clone(),
            script: crate::account_script::Script::attach(view)?,
            forbidden,
            eligible: Arc::new(eligible),
            busy: AtomicBool::new(false),
        }))
    }
    fn active(&self) -> bool {
        (self.eligible)()
    }
    fn report(&self, message: &str) {
        let _ = self
            .view
            .app_handle()
            .emit_to("main", "account-error", message);
    }
    fn begin(&self) -> bool {
        self.active()
            && self
                .busy
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
    }
    /// Called only by CEF's native keyboard handler, never by a page message.
    pub fn paste(self: &Arc<Self>, browser: tauri_runtime_cef::cef::Browser) -> bool {
        if !browser
            .focused_frame()
            .is_some_and(|frame| frame.is_main() != 0)
        {
            return true;
        }
        if !browser.main_frame().is_some_and(|frame| {
            tauri::Url::parse(&CefString::from(&frame.url()).to_string())
                .ok()
                .is_some_and(|url| crate::whatsapp::is_origin(&url))
        }) {
            return true;
        }
        if !self.begin() {
            return true;
        }
        let this = self.clone();
        std::thread::spawn(move || {
            let object = this.capture();
            if !this.active() {
                if let Some(object) = object {
                    this.cleanup(&object);
                }
                this.busy.store(false, Ordering::Release);
                return;
            }
            match this.read_clipboard() {
                Ok(Some(paths)) => {
                    let result = object
                        .as_deref()
                        .context("Click the message box and paste again")
                        .and_then(|object| this.deliver(object, paths, None));
                    if result.is_err() && this.active() {
                        this.report("Could not attach copied files. Use local files, up to 100 files and 2 GB in total, and paste into the message box.");
                    }
                }
                Ok(None) => {
                    if this.active() {
                        let active = this.clone();
                        let _ = this.view.with_cef_webview(move |native| {
                            if active.active() && native.snapshot().visible == Some(true) {
                                if let Some(frame) = native.browser().main_frame() {
                                    frame.paste();
                                }
                            }
                        });
                    }
                }
                Err(_) => {
                    if this.active() {
                        this.report("Could not read copied files. Copy them again and retry.");
                    }
                }
            }
            if let Some(object) = object {
                this.cleanup(&object);
            }
            this.busy.store(false, Ordering::Release);
        });
        true
    }
    /// Coordinates and paths must come from the native window's OS drop event.
    pub fn drop_at(self: &Arc<Self>, paths: Vec<PathBuf>, position: tauri::PhysicalPosition<f64>) {
        if !self.active() {
            return;
        }
        if let (Ok(origin), Ok(size)) = (self.view.position(), self.view.size()) {
            if size.width > 0 && size.height > 0 {
                self.drop_files(
                    paths,
                    [
                        (position.x - origin.x as f64) / size.width as f64,
                        (position.y - origin.y as f64) / size.height as f64,
                    ],
                );
            }
        }
    }

    /// Accept only paths from the native window's OS drop event.
    pub fn drop_files(self: &Arc<Self>, paths: Vec<PathBuf>, point: [f64; 2]) {
        if !point
            .iter()
            .all(|value| value.is_finite() && (0. ..1.).contains(value))
            || !self.begin()
        {
            return;
        }
        let _ = self.view.set_focus();
        let this = self.clone();
        std::thread::spawn(move || {
            let object = this.capture();
            let result = object
                .as_deref()
                .context("Conversation unavailable")
                .and_then(|object| this.deliver(object, paths, Some(point)));
            if result.is_err() && this.active() {
                this.report("Could not attach dropped files. Open a conversation and use local files, up to 100 files and 2 GB in total.");
            }
            if let Some(object) = object {
                this.cleanup(&object);
            }
            this.busy.store(false, Ordering::Release);
        });
    }
    fn capture(&self) -> Option<String> {
        if !self.active() {
            return None;
        }
        self.script.evaluate_remote(&self.view, CAPTURE, false)?["result"]["objectId"]
            .as_str()
            .map(str::to_owned)
    }
    fn cleanup(&self, object: &str) {
        let _ = self.script.command(
            &self.view,
            "Runtime.callFunctionOn",
            json!({"objectId":object,"functionDeclaration":CLEANUP}),
        );
        let _ = self.script.command(
            &self.view,
            "Runtime.releaseObject",
            json!({"objectId":object}),
        );
    }
    fn deliver(&self, object: &str, paths: Vec<PathBuf>, point: Option<[f64; 2]>) -> Result<()> {
        let paths = validated_paths(paths, &self.forbidden)?;
        ensure!(self.active(), "Account changed");
        self.script
            .command(
                &self.view,
                "DOM.setFileInputFiles",
                json!({"objectId":object,"files":paths}),
            )
            .context("Could not stage selected files")?;
        ensure!(self.active(), "Account changed");
        let result = self.script.command(&self.view, "Runtime.callFunctionOn", json!({"objectId":object,"functionDeclaration":DISPATCH,"arguments":[{"value":if point.is_some(){"drop"}else{"paste"}},{"value":point}],"returnByValue":true,"awaitPromise":true})).context("Could not prepare attachments")?;
        ensure!(result["result"]["value"] == true, "Conversation changed");
        Ok(())
    }
    fn read_clipboard(&self) -> Result<Option<Vec<PathBuf>>> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let eligible = self.eligible.clone();
        self.view.app_handle().run_on_main_thread(move || {
            if !eligible() {
                let _ = sender.send(Err(anyhow::anyhow!("Account changed")));
                return;
            }
            let Some(display) = gtk::gdk::Display::default() else {
                let _ = sender.send(Err(anyhow::anyhow!("Clipboard unavailable")));
                return;
            };
            let clipboard = display.clipboard();
            let types = ["x-special/gnome-copied-files", "text/uri-list"];
            if !types
                .iter()
                .any(|mime| clipboard.formats().contain_mime_type(mime))
            {
                let _ = sender.send(Ok(None));
                return;
            }
            glib::MainContext::default().spawn_local(async move {
                let operation = async {
                    let (stream, mime) = clipboard
                        .read_future(&types, glib::Priority::DEFAULT)
                        .await?;
                    let mut bytes = Vec::new();
                    loop {
                        let block = stream
                            .read_bytes_future(4096, glib::Priority::DEFAULT)
                            .await?;
                        if block.is_empty() {
                            break;
                        }
                        ensure!(
                            bytes.len() + block.len() <= MAX_URI_BYTES,
                            "Too many copied files"
                        );
                        bytes.extend_from_slice(&block);
                    }
                    Ok(Some(clipboard_paths(&bytes, &mime)?))
                };
                let result =
                    match glib::future_with_timeout(Duration::from_secs(3), operation).await {
                        Ok(result) => result,
                        Err(_) => Err(anyhow::anyhow!("Clipboard read timed out")),
                    };
                let _ = sender.send(result);
            });
        })?;
        receiver
            .recv_timeout(Duration::from_secs(4))
            .context("Clipboard read timed out")?
    }
}

fn clipboard_paths(bytes: &[u8], mime: &str) -> Result<Vec<PathBuf>> {
    ensure!(
        matches!(mime, "text/uri-list" | "x-special/gnome-copied-files"),
        "Unsupported clipboard type"
    );
    ensure!(bytes.len() <= MAX_URI_BYTES, "Copied file list too large");
    let content = std::str::from_utf8(bytes)?;
    let mut lines = content.lines();
    if mime == "x-special/gnome-copied-files" {
        ensure!(
            matches!(lines.next(), Some("copy" | "cut")),
            "Invalid copied file list"
        );
    }
    let mut paths = Vec::new();
    for line in lines.filter(|line| !line.is_empty() && !line.starts_with('#')) {
        ensure!(paths.len() < MAX_FILES, "Too many files");
        let url = tauri::Url::parse(line)?;
        ensure!(
            url.scheme() == "file"
                && url.host_str().is_none_or(|host| host == "localhost")
                && url.query().is_none()
                && url.fragment().is_none(),
            "Only local file URLs are supported"
        );
        paths.push(
            url.to_file_path()
                .map_err(|_| anyhow::anyhow!("Invalid file URL"))?,
        );
    }
    ensure!(!paths.is_empty(), "No copied files");
    Ok(paths)
}
fn validated_paths(paths: Vec<PathBuf>, forbidden: &Path) -> Result<Vec<String>> {
    ensure!(
        !paths.is_empty() && paths.len() <= MAX_FILES,
        "Select between 1 and 100 files"
    );
    let forbidden = forbidden.canonicalize().context("Profile unavailable")?;
    let mut total = 0u64;
    let mut result = Vec::new();
    for path in paths {
        let path = path.canonicalize().context("File unavailable")?;
        ensure!(
            !path.starts_with(&forbidden),
            "Private profile files cannot be attached"
        );
        let name = path.to_str().context("File name is not valid UTF-8")?;
        if result.iter().any(|existing| existing == name) {
            continue;
        }
        let metadata = path.metadata()?;
        ensure!(metadata.is_file(), "Choose files, not folders");
        total = total
            .checked_add(metadata.len())
            .context("Files too large")?;
        ensure!(total <= MAX_BYTES, "Files exceed 2 GB");
        let path = path.to_str().context("File name is not valid UTF-8")?;
        ensure!(
            path.len() <= 4096 && !path.chars().any(char::is_control),
            "Invalid file name"
        );
        if !result.iter().any(|existing| existing == path) {
            result.push(path.to_owned());
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn file_manager_clipboards_accept_only_bounded_local_file_lists() {
        assert_eq!(
            clipboard_paths(
                b"copy\nfile:///tmp/Sample%20image.png\n",
                "x-special/gnome-copied-files"
            )
            .unwrap(),
            vec![PathBuf::from("/tmp/Sample image.png")]
        );
        assert_eq!(
            clipboard_paths(
                b"# Selected files\r\nfile:///tmp/a.zip\r\nfile://localhost/tmp/b.pdf\r\n",
                "text/uri-list"
            )
            .unwrap()
            .len(),
            2
        );
        for list in [
            "https://example.com/a",
            "file://server/share/a",
            "file:///tmp/a?token=secret",
            "file:///tmp/a#fragment",
            "",
            "not a URL",
        ] {
            assert!(clipboard_paths(list.as_bytes(), "text/uri-list").is_err());
        }
        assert!(clipboard_paths(b"delete\nfile:///tmp/a", "x-special/gnome-copied-files").is_err());
        assert!(clipboard_paths(b"file:///tmp/a", "text/plain").is_err());
        assert!(clipboard_paths(&vec![b'a'; MAX_URI_BYTES + 1], "text/uri-list").is_err());
        assert!(clipboard_paths(
            "file:///tmp/a\n".repeat(MAX_FILES + 1).as_bytes(),
            "text/uri-list"
        )
        .is_err());
    }
    #[test]
    fn selected_files_exclude_vault_contents_links_folders_and_oversized_files() {
        let root = tempfile::tempdir().unwrap();
        let vault = root.path().join("vault");
        std::fs::create_dir(&vault).unwrap();
        let private = vault.join("catalog.json");
        std::fs::write(&private, b"synthetic private data").unwrap();
        let selected = root.path().join("sample.zip");
        std::fs::write(&selected, b"synthetic attachment").unwrap();
        let link = root.path().join("profile-link");
        std::os::unix::fs::symlink(&private, &link).unwrap();
        for path in [
            private,
            link,
            root.path().into(),
            root.path().join("missing"),
        ] {
            assert!(validated_paths(vec![path], &vault).is_err());
        }
        let socket_path = root.path().join("synthetic.sock");
        let _socket = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();
        assert!(validated_paths(vec![socket_path], &vault).is_err());
        assert_eq!(
            validated_paths(vec![selected.clone(), selected.clone()], &vault)
                .unwrap()
                .len(),
            1
        );
        std::fs::File::create(&selected)
            .unwrap()
            .set_len(MAX_BYTES + 1)
            .unwrap();
        assert!(validated_paths(vec![selected], &vault).is_err());
        assert!(validated_paths(vec![], &vault).is_err());
    }
}

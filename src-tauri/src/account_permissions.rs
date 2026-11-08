//! Native, account-scoped consent. No page script can grant device access.
use gtk::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri_runtime_cef::{
    cef::{self, *},
    MediaPermissionRequest,
};

const AUDIO: u32 =
    cef::sys::cef_media_access_permission_types_t::CEF_MEDIA_PERMISSION_DEVICE_AUDIO_CAPTURE as u32;
const VIDEO: u32 =
    cef::sys::cef_media_access_permission_types_t::CEF_MEDIA_PERMISSION_DEVICE_VIDEO_CAPTURE as u32;
#[derive(Clone, Copy, Default, Deserialize, Serialize, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    #[default]
    Ask,
    Allow,
    Block,
}
#[derive(Clone, Default, Deserialize, Serialize)]
pub struct Permissions {
    pub microphone: Decision,
    pub camera: Decision,
}
struct State {
    saved: Permissions,
    generation: u64,
    pending: bool,
}
pub struct AccountPermissions {
    path: PathBuf,
    state: Mutex<State>,
}
thread_local! { static PARENT: RefCell<Option<gtk::glib::WeakRef<gtk::Window>>> = const { RefCell::new(None) }; }
pub fn set_parent(window: &gtk::Window) {
    PARENT.with(|parent| *parent.borrow_mut() = Some(window.downgrade()));
}
fn trusted(origin: &str) -> bool {
    tauri::Url::parse(origin)
        .ok()
        .is_some_and(|url| crate::whatsapp::is_origin(&url))
}
fn valid(request: &MediaPermissionRequest) -> bool {
    request.permissions != 0
        && request.permissions & !(AUDIO | VIDEO) == 0
        && request.browser.is_valid() != 0
        && request.frame.is_valid() != 0
        && request.frame.is_main() != 0
        && trusted(&request.origin)
        && trusted(&CefString::from(&request.frame.url()).to_string())
}
fn decision(saved: &Permissions, mask: u32) -> Decision {
    let requested = [(AUDIO, saved.microphone), (VIDEO, saved.camera)];
    if requested
        .iter()
        .any(|(bit, value)| mask & bit != 0 && *value == Decision::Block)
    {
        Decision::Block
    } else if requested
        .iter()
        .any(|(bit, value)| mask & bit != 0 && *value == Decision::Ask)
    {
        Decision::Ask
    } else {
        Decision::Allow
    }
}
pub fn apply(browser: &Browser, saved: &Permissions) {
    if let Some(context) = browser.host().and_then(|host| host.request_context()) {
        let origin = CefString::from("https://web.whatsapp.com/");
        for (kind, value) in [
            (ContentSettingTypes::MEDIASTREAM_MIC, saved.microphone),
            (ContentSettingTypes::MEDIASTREAM_CAMERA, saved.camera),
        ] {
            context.set_content_setting(
                Some(&origin),
                None,
                kind,
                match value {
                    Decision::Allow => ContentSettingValues::ALLOW,
                    Decision::Ask => ContentSettingValues::ASK,
                    Decision::Block => ContentSettingValues::BLOCK,
                },
            );
        }
    }
}
impl AccountPermissions {
    pub fn load(profile: PathBuf) -> anyhow::Result<Arc<Self>> {
        let path = profile.join("media-permissions.json");
        let saved = if path.try_exists()? {
            crate::storage::read(&path)?
        } else {
            Permissions::default()
        };
        Ok(Arc::new(Self {
            path,
            state: Mutex::new(State {
                saved,
                generation: 0,
                pending: false,
            }),
        }))
    }
    pub fn invalidate(&self) {
        self.state.lock().unwrap().generation += 1;
    }
    pub fn snapshot(&self) -> Permissions {
        self.state.lock().unwrap().saved.clone()
    }
    pub fn reset(&self, device: &str, block: bool) -> Result<Permissions, String> {
        let mut state = self.state.lock().unwrap();
        let mut next = state.saved.clone();
        let value = if block {
            Decision::Block
        } else {
            Decision::Ask
        };
        match device {
            "microphone" => next.microphone = value,
            "camera" => next.camera = value,
            _ => return Err("Unknown device permission".into()),
        }
        crate::storage::write(&self.path, &next)
            .map_err(|_| "Could not save device permissions")?;
        state.generation += 1;
        state.saved = next.clone();
        Ok(next)
    }
    pub fn request(self: &Arc<Self>, request: MediaPermissionRequest, name: String) {
        if !valid(&request) {
            return;
        }
        let mut state = self.state.lock().unwrap();
        match decision(&state.saved, request.permissions) {
            Decision::Block => return,
            Decision::Allow => {
                let saved = state.saved.clone();
                drop(state);
                apply(&request.browser, &saved);
                let mask = request.permissions;
                request.respond(mask);
                return;
            }
            Decision::Ask => {}
        }
        if state.pending {
            return;
        }
        state.pending = true;
        let generation = state.generation;
        drop(state);
        let permissions = self.clone();
        gtk::glib::MainContext::default().spawn_local(async move {
            let devices = match request.permissions { AUDIO => "microphone", VIDEO => "camera", _ => "microphone and camera" };
            let dialog = gtk::AlertDialog::builder().message(format!("Allow {devices} for {name}?"))
                .detail("Elo Talks will allow this WhatsApp account (web.whatsapp.com) to use these devices. You can revoke access in Settings → Security. Your choice is stored inside the vault.")
                .buttons(["Don't allow", "Allow"]).cancel_button(0).default_button(0).modal(true).build();
            let parent = PARENT.with(|parent| parent.borrow().as_ref().and_then(|parent| parent.upgrade()));
            let cancel = gtk::gio::Cancellable::new();
            let timeout_cancel = cancel.clone();
            let timer = gtk::glib::timeout_add_local_once(Duration::from_secs(60), move || timeout_cancel.cancel());
            let dialog_parent = parent.clone();
            let dialog_cancel = cancel.clone();
            let result = gtk::gio::GioFuture::new(&dialog, move |dialog, _, sender| {
                dialog.choose(dialog_parent.as_ref(), Some(&dialog_cancel), move |result| sender.resolve(result));
            }).await;
            // The cancellation timer may already have fired.
            if !cancel.is_cancelled() { timer.remove(); }
            let mut state = permissions.state.lock().unwrap();
            state.pending = false;
            if state.generation != generation || !valid(&request) { return; }
            let choice = match result { Ok(1) => Decision::Allow, Ok(_) => Decision::Block, Err(_) => return };
            let mut next = state.saved.clone();
            if request.permissions & AUDIO != 0 { next.microphone = choice; }
            if request.permissions & VIDEO != 0 { next.camera = choice; }
            if crate::storage::write(&permissions.path, &next).is_err() {
                gtk::AlertDialog::builder().message("Could not save device permissions").detail("Access was denied. Check that your vault is available and try again.").build().show(parent.as_ref());
                return;
            }
            state.saved = next.clone();
            drop(state);
            apply(&request.browser, &next);
            let mask = if choice == Decision::Allow { request.permissions } else { 0 };
            request.respond(mask);
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decisions_are_per_device_and_denial_wins() {
        let mut saved = Permissions::default();
        assert_eq!(decision(&saved, AUDIO), Decision::Ask);
        saved.microphone = Decision::Allow;
        assert_eq!(decision(&saved, AUDIO), Decision::Allow);
        assert_eq!(decision(&saved, AUDIO | VIDEO), Decision::Ask);
        saved.camera = Decision::Block;
        assert_eq!(decision(&saved, AUDIO | VIDEO), Decision::Block);
        assert!(!trusted("https://web.whatsapp.com.evil.test/"));
        assert!(!trusted("http://web.whatsapp.com/"));
    }
    #[test]
    fn revocation_is_durable_and_account_isolated() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let a = AccountPermissions::load(first.path().to_owned()).unwrap();
        a.reset("microphone", true).unwrap();
        assert_eq!(
            AccountPermissions::load(first.path().to_owned())
                .unwrap()
                .snapshot()
                .microphone,
            Decision::Block
        );
        assert_eq!(
            AccountPermissions::load(second.path().to_owned())
                .unwrap()
                .snapshot()
                .microphone,
            Decision::Ask
        );
        assert_eq!(a.snapshot().camera, Decision::Ask);
        a.reset("microphone", false).unwrap();
        assert_eq!(a.snapshot().microphone, Decision::Ask);
        assert!(a.reset("screen", false).is_err());
        std::fs::write(first.path().join("media-permissions.json"), "invalid").unwrap();
        assert!(AccountPermissions::load(first.path().to_owned()).is_err());
    }
}

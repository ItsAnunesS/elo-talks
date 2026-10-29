//! Best-effort self-profile pixels. All state belongs to the disposable UI process.
use crate::{
    account_events::{self, AccountEvents},
    account_runtime::AvatarStatus,
};
use gtk::{glib, prelude::*};
use serde::Serialize;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};
use tauri::{Emitter, Manager};
use webkit2gtk::WebViewExt;

const PIXEL_BYTES: usize = 64 * 64 * 4;
const PROBE: &str = concat!("return ", include_str!("avatar-probe.js"));
const WORLD: &str = "dev.anuness.elotalks.avatar";

#[derive(Clone, Serialize)]
struct AvatarEvent<'a> {
    id: &'a str,
    pixels: Option<Vec<u8>>,
}

fn decode_pixels(value: &str) -> Option<Vec<u8>> {
    if value.len() != PIXEL_BYTES * 2 {
        return None;
    }
    hex::decode(value).ok()
}

fn avatar_status(value: &str) -> AvatarStatus {
    if decode_pixels(value).is_some() {
        return AvatarStatus::Ready;
    }
    match value {
        "control-not-found" => AvatarStatus::ControlNotFound,
        "no-photo" => AvatarStatus::NoPhoto,
        "ambiguous" => AvatarStatus::Ambiguous,
        "image-not-ready" => AvatarStatus::ImageNotReady,
        "image-too-large" => AvatarStatus::ImageTooLarge,
        "canvas-blocked" => AvatarStatus::CanvasBlocked,
        "canvas-unavailable" => AvatarStatus::CanvasUnavailable,
        "image-request-failed" => AvatarStatus::ImageRequestFailed,
        "image-source-unsupported" => AvatarStatus::ImageSourceUnsupported,
        "wrong-origin" => AvatarStatus::WrongOrigin,
        _ => AvatarStatus::Unavailable,
    }
}

struct Probe {
    busy: Cell<bool>,
    active: Cell<bool>,
    generation: Cell<u64>,
    previous: RefCell<String>,
    cancellation: RefCell<Option<gtk::gio::Cancellable>>,
}

impl Probe {
    fn clear(&self, app: &tauri::AppHandle, id: &str) {
        self.generation.set(self.generation.get().wrapping_add(1));
        if let Some(cancellation) = self.cancellation.borrow_mut().take() {
            cancellation.cancel();
        }
        self.publish(app, id, String::new(), AvatarStatus::Waiting);
    }

    fn publish(&self, app: &tauri::AppHandle, id: &str, pixels: String, status: AvatarStatus) {
        let changed = {
            let events = app.state::<AccountEvents>();
            let mut runtime = events.runtime.lock().unwrap();
            runtime.accounts.get_mut(id).is_some_and(|account| {
                let changed = account.avatar != status;
                account.avatar = status;
                changed
            })
        };
        if changed {
            account_events::publish(app);
        }
        if *self.previous.borrow() == pixels {
            return;
        }
        let decoded = decode_pixels(&pixels);
        let _ = app.emit_to(
            "main",
            "account-avatar",
            AvatarEvent {
                id,
                pixels: decoded,
            },
        );
        *self.previous.borrow_mut() = pixels;
    }

    fn sample(self: &Rc<Self>, view: &webkit2gtk::WebView, app: &tauri::AppHandle, id: &str) {
        if !self.active.get() {
            return;
        }
        if view
            .uri()
            .is_none_or(|uri| !uri.starts_with("https://web.whatsapp.com/"))
        {
            self.clear(app, id);
            return;
        }
        if self.busy.replace(true) {
            return;
        }
        let probe = self.clone();
        let handle = app.clone();
        let account = id.to_owned();
        let generation = self.generation.get();
        let cancellation = gtk::gio::Cancellable::new();
        *self.cancellation.borrow_mut() = Some(cancellation.clone());
        let deadline = cancellation.clone();
        glib::timeout_add_local_once(Duration::from_secs(7), move || deadline.cancel());
        view.call_async_javascript_function(
            PROBE,
            None,
            Some(WORLD),
            None,
            Some(&cancellation),
            move |result| {
                probe.busy.set(false);
                // A result from a previous navigation must never restore an old user's photo.
                if probe.generation.get() != generation {
                    return;
                }
                probe.cancellation.borrow_mut().take();
                let pixels = result
                    .ok()
                    .map(|value| value.to_string())
                    .unwrap_or_default();
                let status = avatar_status(&pixels);
                let pixels = if status == AvatarStatus::Ready {
                    pixels
                } else {
                    String::new()
                };
                probe.publish(&handle, &account, pixels, status);
            },
        );
    }
}

pub fn attach(view: &webkit2gtk::WebView, app: &tauri::AppHandle, id: &str) {
    let probe = Rc::new(Probe {
        busy: Cell::new(false),
        active: Cell::new(false),
        generation: Cell::new(0),
        previous: RefCell::new(String::new()),
        cancellation: RefCell::new(None),
    });
    let state = probe.clone();
    let handle = app.clone();
    let account = id.to_owned();
    view.connect_load_changed(move |view, event| match event {
        webkit2gtk::LoadEvent::Started => {
            state.active.set(true);
            state.clear(&handle, &account);
        }
        webkit2gtk::LoadEvent::Finished => state.sample(view, &handle, &account),
        _ => {}
    });
    let state = probe.clone();
    let handle = app.clone();
    let account = id.to_owned();
    view.connect_web_process_terminated(move |_, _| {
        state.active.set(false);
        state.clear(&handle, &account);
    });
    let state = probe.clone();
    let handle = app.clone();
    let account = id.to_owned();
    view.connect_load_failed(move |_, _, _, _| {
        state.active.set(false);
        state.clear(&handle, &account);
        false
    });
    let weak = view.downgrade();
    let handle = app.clone();
    let account = id.to_owned();
    // Bounded polling catches login, logout and photo changes in the single-page app.
    // Weak ownership stops the timer when its account webview is destroyed.
    glib::timeout_add_local(Duration::from_secs(15), move || {
        let Some(view) = weak.upgrade() else {
            return glib::ControlFlow::Break;
        };
        probe.sample(&view, &handle, &account);
        glib::ControlFlow::Continue
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_only_fixed_size_raw_pixels() {
        assert_eq!(
            decode_pixels(&"ab".repeat(PIXEL_BYTES)).unwrap().len(),
            PIXEL_BYTES
        );
        for invalid in [
            String::new(),
            "data:image/svg+xml,<svg/>".into(),
            "https://example.com/photo".into(),
            "zz".repeat(PIXEL_BYTES),
            "00".repeat(PIXEL_BYTES + 1),
        ] {
            assert!(decode_pixels(&invalid).is_none());
        }
    }
    #[test]
    fn diagnostics_accept_only_known_status_codes() {
        assert_eq!(avatar_status("canvas-blocked"), AvatarStatus::CanvasBlocked);
        assert_eq!(
            avatar_status("control-not-found"),
            AvatarStatus::ControlNotFound
        );
        assert_eq!(
            avatar_status("private URL or arbitrary page text"),
            AvatarStatus::Unavailable
        );
    }
}

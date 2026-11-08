//! Bounded self-avatar adapter with an optional cache inside the unlocked vault.
use crate::{account_events::AccountEvents, account_runtime::AvatarState};
use gtk::{gdk_pixbuf::PixbufLoader, glib, prelude::*};

use tauri::{Emitter, Manager};

use std::collections::VecDeque;
use std::sync::{mpsc, Arc, Mutex};
use tauri_runtime_cef::{allocate_devtools_message_id, DevToolsProtocol, WebviewCefExt};
#[derive(Default)]
pub struct Resources {
    images: Mutex<VecDeque<(String, String)>>,
    pending: Mutex<Option<(i32, mpsc::SyncSender<Vec<u8>>)>>,
}
pub fn observe(view: &tauri::Webview, app: &tauri::AppHandle, id: &str) -> tauri::Result<()> {
    let resources = Arc::new(Resources::default());
    app.state::<AccountEvents>()
        .avatar_resources
        .lock()
        .unwrap()
        .insert(id.to_owned(), resources.clone());
    view.on_dev_tools_protocol(move |event| match event {
        DevToolsProtocol::Event { method, params }
            if method == "Network.responseReceived" && params.len() <= 32 * 1024 =>
        {
            let Ok(value) = serde_json::from_slice::<serde_json::Value>(&params) else {
                return;
            };
            let response = &value["response"];
            let Some(url) = response["url"]
                .as_str()
                .filter(|url| url.len() <= 4096 && eligible_image_resource(url))
            else {
                return;
            };
            if !matches!(
                response["mimeType"].as_str(),
                Some("image/jpeg" | "image/png")
            ) {
                return;
            }
            let Some(request) = value["requestId"].as_str().filter(|id| id.len() <= 256) else {
                return;
            };
            let mut images = resources.images.lock().unwrap();
            if images.len() >= 64 {
                images.pop_front();
            }
            images.push_back((url.to_owned(), request.to_owned()));
        }
        DevToolsProtocol::MethodResult {
            message_id,
            success,
            result,
        } => {
            let mut pending = resources.pending.lock().unwrap();
            if pending.as_ref().is_none_or(|(id, _)| *id != message_id) {
                return;
            }
            let (_, sender) = pending.take().unwrap();
            if !success || result.len() > MAX_BYTES * 2 {
                return;
            }
            if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&result) {
                if value["base64Encoded"] == true {
                    if let Some(body) = value["body"].as_str() {
                        let bytes = glib::base64_decode(body);
                        if bytes.len() <= MAX_BYTES {
                            let _ = sender.try_send(bytes);
                        }
                    }
                }
            }
        }
        _ => {}
    })?;
    let message_id = allocate_devtools_message_id().map_err(anyhow::Error::from)?;
    view.send_dev_tools_message(serde_json::json!({"id":message_id,"method":"Network.enable","params":{"maxTotalBufferSize":MAX_BYTES * 64,"maxResourceBufferSize":MAX_BYTES}}).to_string().as_bytes())
}
fn remote_image(view: &tauri::Webview, resources: &Resources, source: &str) -> Option<String> {
    let request = resources
        .images
        .lock()
        .unwrap()
        .iter()
        .rev()
        .find(|(url, _)| url == source)
        .map(|(_, request)| request.clone())?;
    let message_id = allocate_devtools_message_id().ok()?;
    let (sender, receiver) = mpsc::sync_channel(1);
    *resources.pending.lock().unwrap() = Some((message_id, sender));
    let result = view.send_dev_tools_message(serde_json::json!({"id":message_id,"method":"Network.getResponseBody","params":{"requestId":request}}).to_string().as_bytes());
    let bytes = if result.is_ok() {
        receiver
            .recv_timeout(std::time::Duration::from_secs(3))
            .ok()
    } else {
        None
    };
    resources.pending.lock().unwrap().take();
    normalized_image(&bytes?)
}

#[derive(Clone)]
pub struct Avatar {
    image: String,
}

const MAX_BYTES: usize = 256 * 1024;
const MAX_ATTEMPTS: u8 = 7;
#[derive(Default)]
struct ProbeBudget {
    attempts: u8,
}
impl ProbeBudget {
    fn exhausted(&self) -> bool {
        self.attempts >= MAX_ATTEMPTS
    }
    fn begin(&mut self) -> bool {
        if self.exhausted() {
            return false;
        }
        self.attempts += 1;
        true
    }
}

#[derive(serde::Deserialize)]
struct Candidate {
    source: String,
    png: Option<String>,
}
fn candidate(value: &str) -> Option<Candidate> {
    if value.len() > 50_000 {
        return None;
    }
    let candidate: Candidate = serde_json::from_str(value).ok()?;
    if candidate.source.len() > 4096 {
        return None;
    }
    Some(candidate)
}
fn blob_image(candidate: &Candidate) -> Option<String> {
    let origin = tauri::Url::parse(candidate.source.strip_prefix("blob:")?).ok()?;
    if !crate::whatsapp::is_origin(&origin) {
        return None;
    }
    let png = candidate.png.as_deref()?;
    if png.len() > 44_000 {
        return None;
    }
    let raw = glib::base64_decode(png.strip_prefix("data:image/png;base64,")?);
    normalized_image(&raw)
}

// This is a filter for existing browser resources, never a URL download policy.
// Only the exact source selected from WhatsApp's self control can be read below.
fn eligible_image_resource(uri: &str) -> bool {
    let Ok(url) = tauri::Url::parse(uri) else {
        return false;
    };
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port_or_known_default() == Some(443)
        && url.host_str().is_some()
}

pub fn normalized_image(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() || bytes.len() > MAX_BYTES {
        return None;
    }
    let format = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "png"
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        "jpeg"
    } else {
        return None;
    };
    let loader = PixbufLoader::with_type(format).ok()?;
    loader.set_size(64, 64);
    let written = loader.write(bytes);
    let closed = loader.close();
    written.ok()?;
    closed.ok()?;
    let image = loader.pixbuf()?;
    if image.width() != 64 || image.height() != 64 {
        return None;
    }
    let png = image.save_to_bufferv("png", &[]).ok()?;
    if png.len() > 32 * 1024 {
        return None;
    }
    Some(format!(
        "data:image/png;base64,{}",
        glib::base64_encode(&png)
    ))
}

fn report(app: &tauri::AppHandle, id: &str, status: AvatarState, active: Option<bool>) {
    if let Some(health) = app
        .state::<AccountEvents>()
        .runtime
        .lock()
        .unwrap()
        .accounts
        .get_mut(id)
    {
        health.avatar = status;
        if let Some(active) = active {
            health.avatar_probe_active = active;
        }
    }
}

fn publish(app: &tauri::AppHandle, id: &str, image: Option<Avatar>) {
    let available = image.is_some();
    if available {
        report(app, id, AvatarState::Ready, Some(false));
    }
    let events = app.state::<AccountEvents>();
    if let Some(path) = events.avatar_cache_paths.lock().unwrap().get(id) {
        if let Some(ref avatar) = image {
            // The UI process exits before the vault unmounts. Failure keeps the RAM fallback.
            let unchanged = events
                .avatars
                .lock()
                .unwrap()
                .get(id)
                .is_some_and(|previous| previous.image == avatar.image);
            if !unchanged {
                let _ = crate::storage::write(path, &avatar.image);
            }
        } else {
            let _ = std::fs::remove_file(path);
        }
    }
    let mut avatars = events.avatars.lock().unwrap();
    if let Some(image) = image {
        avatars.insert(id.to_owned(), image);
    } else {
        avatars.remove(id);
    }
    drop(avatars);
    let _ = app.emit_to(
        "main",
        "account-avatar",
        serde_json::json!({"id":id,"available":available}),
    );
}

/// Restore only a bounded, normalized image, never a remote URL or browser identifier.
pub fn restore(app: &tauri::AppHandle, id: &str, path: std::path::PathBuf) {
    let events = app.state::<AccountEvents>();
    let cached = read_cache(&path);
    events
        .avatar_cache_paths
        .lock()
        .unwrap()
        .insert(id.to_owned(), path);
    if let Some(image) = cached {
        events
            .avatars
            .lock()
            .unwrap()
            .insert(id.to_owned(), Avatar { image });
        let _ = app.emit_to(
            "main",
            "account-avatar",
            serde_json::json!({"id": id, "available": true}),
        );
    }
}

fn read_cache(path: &std::path::Path) -> Option<String> {
    let encoded: String = crate::storage::read(path).ok()?;
    if encoded.len() > 44_000 {
        return None;
    }
    let bytes = glib::base64_decode(encoded.strip_prefix("data:image/png;base64,")?);
    normalized_image(&bytes)
}

pub fn clear(app: &tauri::AppHandle, id: &str) {
    publish(app, id, None);
}

/// Return the last known photo while the bounded background probe refreshes it.
pub fn current(app: &tauri::AppHandle, id: &str) -> Option<String> {
    app.state::<AccountEvents>()
        .avatars
        .lock()
        .unwrap()
        .get(id)
        .map(|avatar| avatar.image.clone())
}

pub fn attach(view: &tauri::Webview, app: &tauri::AppHandle, id: &str) {
    let generation = {
        let events = app.state::<AccountEvents>();
        let mut runtime = events.runtime.lock().unwrap();
        let Some(health) = runtime.accounts.get_mut(id) else {
            return;
        };
        if health.avatar_probe_active {
            return;
        }
        health.avatar_probe_active = true;
        health.avatar = AvatarState::Searching;
        health.generation
    };
    let Some(script) = app
        .state::<AccountEvents>()
        .scripts
        .lock()
        .unwrap()
        .get(id)
        .cloned()
    else {
        return;
    };
    let view = view.clone();
    let handle = app.clone();
    let account = id.to_owned();
    let resources = app
        .state::<AccountEvents>()
        .avatar_resources
        .lock()
        .unwrap()
        .get(id)
        .cloned();
    std::thread::spawn(move || {
        let mut budget = ProbeBudget::default();
        let mut status = AvatarState::ControlNotFound;
        let mut found = None;
        while budget.begin() {
            std::thread::sleep(std::time::Duration::from_secs(2));
            {
                let events = handle.state::<AccountEvents>();
                let mut runtime = events.runtime.lock().unwrap();
                let Some(health) = runtime
                    .accounts
                    .get_mut(&account)
                    .filter(|health| health.generation == generation)
                else {
                    return;
                };
                health.avatar_attempts = budget.attempts;
            }
            if !view
                .url()
                .ok()
                .is_some_and(|u| crate::whatsapp::is_origin(&u))
            {
                break;
            }
            let Some(result) = script.evaluate(&view, include_str!("avatar.js")) else {
                status = AvatarState::ReadFailed;
                continue;
            };
            let Some(value) = result.as_str().and_then(candidate) else {
                status = AvatarState::ControlNotFound;
                continue;
            };
            found = blob_image(&value).or_else(|| {
                resources
                    .as_ref()
                    .and_then(|resources| remote_image(&view, resources, &value.source))
            });
            status = if value.png.is_some() {
                AvatarState::DecodeFailed
            } else if value.source.starts_with("blob:") {
                AvatarState::UnsupportedImage
            } else {
                AvatarState::ResourceUnavailable
            };
            if found.is_some() {
                break;
            }
        }
        let completion = handle.clone();
        // Publish on the same native event loop that advances navigation state.
        let _ = handle.run_on_main_thread(move || {
            {
                let events = completion.state::<AccountEvents>();
                let mut runtime = events.runtime.lock().unwrap();
                let Some(health) = runtime
                    .accounts
                    .get_mut(&account)
                    .filter(|health| health.generation == generation)
                else {
                    return;
                };
                health.avatar_probe_active = false;
                health.avatar = status;
            }
            if let Some(image) = found {
                publish(&completion, &account, Some(Avatar { image }));
            }
            crate::account_events::publish(&completion);
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn seven_attempts_stop_until_a_new_navigation_cycle() {
        let mut cycle = ProbeBudget::default();
        for expected in 1..=7 {
            assert!(cycle.begin());
            assert_eq!(cycle.attempts, expected);
        }
        assert!(cycle.exhausted());
        assert!(!cycle.begin());
        assert_eq!(cycle.attempts, 7);
        let mut reloaded = ProbeBudget::default();
        assert!(reloaded.begin());
        assert_eq!(reloaded.attempts, 1);
    }

    #[test]
    fn cache_restores_only_bounded_normalized_images() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("self-avatar.json");
        assert!(read_cache(&path).is_none());
        let pixel = gtk::gdk_pixbuf::Pixbuf::new(gtk::gdk_pixbuf::Colorspace::Rgb, true, 8, 64, 64)
            .unwrap();
        pixel.fill(0xaabbccff);
        let image = normalized_image(&pixel.save_to_bufferv("png", &[]).unwrap()).unwrap();
        crate::storage::write(&path, &image).unwrap();
        assert_eq!(read_cache(&path), Some(image));
        for invalid in [
            "https://example.com/photo".to_owned(),
            "data:image/png;base64,broken".to_owned(),
            "x".repeat(44_001),
        ] {
            crate::storage::write(&path, &invalid).unwrap();
            assert!(read_cache(&path).is_none());
        }
    }

    #[test]
    fn resource_policy_and_raster_limits() {
        assert!(eligible_image_resource(
            "https://pps.whatsapp.net/v/photo.jpg"
        ));
        assert!(eligible_image_resource(
            "https://web.whatsapp.com/profile-image"
        ));
        assert!(eligible_image_resource(
            "https://avatar-cdn.example/photo.jpg"
        ));
        // Eligibility alone never authorizes reading: the resource must also match
        // the unique self-profile control in the exact WhatsApp origin.

        for uri in [
            "http://pps.whatsapp.net/a",
            "file:///tmp/photo",
            "https://pps.whatsapp.net:8443/a",
            "https://user@pps.whatsapp.net/a",
        ] {
            assert!(!eligible_image_resource(uri));
        }
        assert!(normalized_image(b"<svg xmlns='http://www.w3.org/2000/svg'/>").is_none());
        assert!(normalized_image(&vec![0; MAX_BYTES + 1]).is_none());
        assert!(normalized_image(b"\x89PNG\r\n\x1a\nbroken").is_none());
        let pixel = gtk::gdk_pixbuf::Pixbuf::new(gtk::gdk_pixbuf::Colorspace::Rgb, true, 8, 32, 32)
            .unwrap();
        pixel.fill(0xaabbccff);
        let bytes = pixel.save_to_bufferv("png", &[]).unwrap();
        let image = normalized_image(&bytes).unwrap();
        assert!(image.starts_with("data:image/png;base64,"));
        assert!(image.len() < 44 * 1024);
        let mut selected = Candidate {
            source: "blob:https://web.whatsapp.com/synthetic".into(),
            png: Some(image),
        };
        assert!(blob_image(&selected).is_some());
        selected.source = "blob:https://evil.test/synthetic".into();
        assert!(blob_image(&selected).is_none());
        assert!(candidate(&"x".repeat(50_001)).is_none());
        assert!(candidate(r#"{"source":"x","png":42}"#).is_none());
    }
}

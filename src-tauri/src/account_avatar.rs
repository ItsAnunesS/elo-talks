//! Bounded self-avatar adapter with an optional cache inside the unlocked vault.
use crate::{account_events::AccountEvents, account_runtime::AvatarState};
use gtk::{gdk_pixbuf::PixbufLoader, glib, prelude::*};
use javascriptcore::ValueExt;
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager};
use webkit2gtk::{URIResponseExt, WebResourceExt, WebViewExt};

#[derive(Clone)]
pub struct Avatar {
    image: String,
}

const MAX_BYTES: usize = 256 * 1024;
const MAX_RESOURCES: usize = 64;
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

fn supported_image(resource: &webkit2gtk::WebResource) -> bool {
    resource.response().is_some_and(|response| {
        response.content_length() > 0
            && response.content_length() <= MAX_BYTES as u64
            && matches!(
                response.mime_type().as_deref(),
                Some("image/jpeg" | "image/png")
            )
    })
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
fn stop_report(app: &tauri::AppHandle, id: &str) {
    if let Some(health) = app
        .state::<AccountEvents>()
        .runtime
        .lock()
        .unwrap()
        .accounts
        .get_mut(id)
    {
        health.avatar_probe_active = false;
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

pub fn attach(view: &webkit2gtk::WebView, app: &tauri::AppHandle, id: &str) {
    let resources = Rc::new(RefCell::new(VecDeque::<webkit2gtk::WebResource>::new()));
    let generation = Rc::new(Cell::new(0u64));
    let collecting = Rc::new(Cell::new(false));
    let cache = Rc::downgrade(&resources);
    let active = collecting.clone();
    let current_generation = generation.clone();
    view.connect_resource_load_started(move |_, resource, _| {
        if !active.get()
            || !resource
                .uri()
                .is_some_and(|uri| eligible_image_resource(&uri))
        {
            return;
        }
        let cache = cache.clone();
        let active = active.clone();
        let generation = current_generation.clone();
        let epoch = generation.get();
        resource.connect_finished(move |resource| {
            if !active.get() || generation.get() != epoch || !supported_image(resource) {
                return;
            }
            let Some(cache) = cache.upgrade() else {
                return;
            };
            let mut cache = cache.borrow_mut();
            if cache.len() == MAX_RESOURCES {
                cache.pop_front();
            }
            cache.push_back(resource.clone());
        });
    });
    let app = app.clone();
    let id = id.to_owned();
    view.connect_load_changed(move |view, event| {
        if event != webkit2gtk::LoadEvent::Started {
            return;
        }
        generation.set(generation.get().wrapping_add(1));
        let epoch = generation.get();
        resources.borrow_mut().clear();
        collecting.set(true);
        report(&app, &id, AvatarState::Searching, Some(true));
        let weak = view.downgrade();
        let app = app.clone();
        let id = id.clone();
        let generation = generation.clone();
        let resources = resources.clone();
        let collecting = collecting.clone();
        let pending = Rc::new(Cell::new(false));
        let done = Rc::new(Cell::new(false));
        let started = Instant::now();
        let mut budget = ProbeBudget::default();
        if let Some(health) = app
            .state::<AccountEvents>()
            .runtime
            .lock()
            .unwrap()
            .accounts
            .get_mut(&id)
        {
            health.avatar_attempts = 0;
        }
        let cancellation = gtk::gio::Cancellable::new();
        glib::timeout_add_local(Duration::from_secs(5), move || {
            if generation.get() != epoch {
                cancellation.cancel();
                return glib::ControlFlow::Break;
            }
            let view = weak.upgrade();
            if view.is_none()
                || done.get()
                || (budget.exhausted() && !pending.get())
                || started.elapsed() >= Duration::from_secs(40)
            {
                cancellation.cancel();
                collecting.set(false);
                if !done.get() {
                    resources.borrow_mut().clear();
                }
                stop_report(&app, &id);
                return glib::ControlFlow::Break;
            }
            let view = view.unwrap();
            if pending.get()
                || !view
                    .uri()
                    .and_then(|uri| tauri::Url::parse(&uri).ok())
                    .is_some_and(|url| crate::whatsapp::is_origin(&url))
            {
                return glib::ControlFlow::Continue;
            }
            if !budget.begin() {
                return glib::ControlFlow::Continue;
            }
            if let Some(health) = app
                .state::<AccountEvents>()
                .runtime
                .lock()
                .unwrap()
                .accounts
                .get_mut(&id)
            {
                health.avatar_attempts = budget.attempts;
            }
            pending.set(true);
            let pending = pending.clone();
            let done = done.clone();
            let generation = generation.clone();
            let resources = resources.clone();
            let app = app.clone();
            let id = id.clone();
            let cancel = cancellation.clone();
            view.evaluate_javascript(
                include_str!("avatar.js"),
                None,
                Some("elo-self-avatar"),
                Some(&cancellation),
                move |result| {
                    if generation.get() != epoch || cancel.is_cancelled() {
                        pending.set(false);
                        return;
                    }
                    let Some(selected) = result
                        .ok()
                        .filter(|v| v.is_string())
                        .and_then(|v| candidate(&v.to_string()))
                    else {
                        report(&app, &id, AvatarState::ControlNotFound, None);
                        pending.set(false);
                        return;
                    };
                    let source = selected.source.clone();
                    if let Some(image) = blob_image(&selected) {
                        pending.set(false);
                        done.set(true);
                        resources.borrow_mut().clear();
                        publish(&app, &id, Some(Avatar { image }));
                        return;
                    }
                    if !eligible_image_resource(&source) {
                        report(&app, &id, AvatarState::UnsupportedImage, None);
                        pending.set(false);
                        return;
                    }
                    let resource = resources
                        .borrow()
                        .iter()
                        .find(|resource| resource.uri().as_deref() == Some(source.as_str()))
                        .cloned();
                    let Some(resource) = resource else {
                        report(&app, &id, AvatarState::ResourceUnavailable, None);
                        pending.set(false);
                        return;
                    };
                    if !supported_image(&resource) {
                        report(&app, &id, AvatarState::UnsupportedImage, None);
                        pending.set(false);
                        return;
                    }
                    let finished_cancel = cancel.clone();
                    WebResourceExt::data(&resource, Some(&cancel), move |data| {
                        pending.set(false);
                        if generation.get() != epoch || finished_cancel.is_cancelled() {
                            return;
                        }
                        let status = if data.is_err() {
                            AvatarState::ReadFailed
                        } else {
                            AvatarState::DecodeFailed
                        };
                        if let Some(image) = data.ok().and_then(|data| normalized_image(&data)) {
                            done.set(true);
                            resources.borrow_mut().retain(|resource| {
                                resource.uri().as_deref() == Some(source.as_str())
                            });
                            publish(&app, &id, Some(Avatar { image }));
                        } else {
                            report(&app, &id, status, None);
                        }
                    });
                },
            );
            glib::ControlFlow::Continue
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

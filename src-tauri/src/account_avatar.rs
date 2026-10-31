//! Memory-only self-avatar adapter. Never downloads a URL or reads other resource bodies.
use crate::account_events::AccountEvents;
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
    source: String,
    image: String,
}

const MAX_BYTES: usize = 256 * 1024;
const MAX_RESOURCES: usize = 64;

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

fn publish(app: &tauri::AppHandle, id: &str, image: Option<Avatar>) {
    let available = image.is_some();
    let events = app.state::<AccountEvents>();
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

/// Recheck the self control when a card is rendered, so a logged-out or changed
/// profile does not keep showing a previous user's in-memory image.
pub fn current(
    view: &webkit2gtk::WebView,
    app: &tauri::AppHandle,
    id: &str,
    reply: impl FnOnce(Option<String>) + 'static,
) {
    let saved = app
        .state::<AccountEvents>()
        .avatars
        .lock()
        .unwrap()
        .get(id)
        .cloned();
    let Some(saved) = saved else {
        reply(None);
        return;
    };
    if !view
        .uri()
        .and_then(|uri| tauri::Url::parse(&uri).ok())
        .is_some_and(|url| crate::whatsapp::is_origin(&url))
    {
        reply(None);
        return;
    }
    let app = app.clone();
    let id = id.to_owned();
    view.evaluate_javascript(
        include_str!("avatar.js"),
        None,
        Some("elo-self-avatar"),
        None::<&gtk::gio::Cancellable>,
        move |result| {
            let same = result
                .ok()
                .filter(|v| v.is_string())
                .and_then(|v| candidate(&v.to_string()))
                .is_some_and(|candidate| candidate.source == saved.source);
            if same {
                reply(Some(saved.image));
            } else {
                publish(&app, &id, None);
                reply(None);
            }
        },
    );
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
        publish(&app, &id, None);
        let weak = view.downgrade();
        let app = app.clone();
        let id = id.clone();
        let generation = generation.clone();
        let resources = resources.clone();
        let collecting = collecting.clone();
        let pending = Rc::new(Cell::new(false));
        let done = Rc::new(Cell::new(false));
        let started = Instant::now();
        let cancellation = gtk::gio::Cancellable::new();
        glib::timeout_add_local(Duration::from_secs(2), move || {
            if generation.get() != epoch {
                cancellation.cancel();
                return glib::ControlFlow::Break;
            }
            let view = weak.upgrade();
            if view.is_none() || done.get() || started.elapsed() >= Duration::from_secs(40) {
                cancellation.cancel();
                collecting.set(false);
                resources.borrow_mut().clear();
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
                        pending.set(false);
                        return;
                    };
                    let source = selected.source.clone();
                    if let Some(image) = blob_image(&selected) {
                        pending.set(false);
                        done.set(true);
                        resources.borrow_mut().clear();
                        publish(&app, &id, Some(Avatar { source, image }));
                        return;
                    }
                    if !eligible_image_resource(&source) {
                        pending.set(false);
                        return;
                    }
                    let resource = resources
                        .borrow()
                        .iter()
                        .find(|resource| resource.uri().as_deref() == Some(source.as_str()))
                        .cloned();
                    let Some(resource) = resource else {
                        pending.set(false);
                        return;
                    };
                    if !supported_image(&resource) {
                        pending.set(false);
                        return;
                    }
                    let finished_cancel = cancel.clone();
                    WebResourceExt::data(&resource, Some(&cancel), move |data| {
                        pending.set(false);
                        if generation.get() != epoch || finished_cancel.is_cancelled() {
                            return;
                        }
                        if let Some(image) = data.ok().and_then(|data| normalized_image(&data)) {
                            done.set(true);
                            resources.borrow_mut().clear();
                            publish(&app, &id, Some(Avatar { source, image }));
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

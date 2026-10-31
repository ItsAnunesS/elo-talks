//! Interactive, ephemeral self-avatar diagnosis. Never writes page content or URLs.
use gtk::{glib, prelude::*};
use javascriptcore::ValueExt;
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
    time::{Duration, Instant},
};
use webkit2gtk::{SettingsExt, URIResponseExt, WebContextExt, WebResourceExt, WebViewExt};

fn main() {
    gtk::init().expect("GTK display unavailable");
    let report = std::env::args().nth(1).expect("Missing report path");
    let context = webkit2gtk::WebContext::new_ephemeral();
    assert!(context.is_sandbox_enabled());
    let view = webkit2gtk::WebView::with_context(&context);
    let settings = WebViewExt::settings(&view).unwrap();
    settings.set_enable_developer_extras(false);
    let resources = Rc::new(RefCell::new(VecDeque::<webkit2gtk::WebResource>::new()));
    let cache = resources.clone();
    view.connect_resource_load_started(move |_, resource, _| {
        let mut items = cache.borrow_mut();
        if items.len() >= 128 {
            items.pop_front();
        }
        items.push_back(resource.clone());
    });
    view.connect_create(|_, _| None);
    view.connect_permission_request(|_, request| {
        webkit2gtk::PermissionRequestExt::deny(request);
        true
    });
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("Elo Talks — isolated avatar test (memory-only session)");
    window.set_default_size(1200, 850);
    let layout = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let notice = gtk::Label::new(Some(
        "TEST ACCOUNT ONLY · Session disappears when closed · No messages are inspected",
    ));
    layout.pack_start(&notice, false, false, 8);
    let preview = gtk::Image::new();
    layout.pack_start(&preview, false, false, 0);
    layout.pack_start(&view, true, true, 0);
    window.add(&layout);
    window.connect_delete_event(|_, _| {
        gtk::main_quit();
        glib::Propagation::Proceed
    });
    let adapter = include_str!("../src/avatar.js");
    let inspection = include_str!("support/avatar_probe.js");
    let script = format!("JSON.stringify({{selected: {adapter}, structural: {inspection}}})");
    let pending = Rc::new(Cell::new(false));
    let weak = view.downgrade();
    let started = Instant::now();
    let converted = Rc::new(RefCell::new((String::new(), serde_json::Value::Null)));
    let adapter_path = std::env::args().nth(2);
    glib::timeout_add_local(Duration::from_secs(3), move || {
        let Some(view) = weak.upgrade() else {
            return glib::ControlFlow::Break;
        };
        if pending.replace(true) {
            return glib::ControlFlow::Continue;
        }
        let script = adapter_path
            .as_deref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|adapter| {
                format!("JSON.stringify({{selected: {adapter}, structural: {inspection}}})")
            })
            .unwrap_or_else(|| script.clone());
        let report = report.clone();
        let converted = converted.clone();
        let preview = preview.clone();
        let pending = pending.clone();
        let resources = resources.clone();
        view.evaluate_javascript(&script, None, Some("elo-self-avatar"), None::<&gtk::gio::Cancellable>, move |result| {
            pending.set(false);
            let Some(mut data) = result.ok().filter(|v| v.is_string()).and_then(|v| serde_json::from_str::<serde_json::Value>(&v.to_string()).ok()) else { return; };
            let selected = data["selected"].as_str().and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok());
            let source = selected.as_ref().and_then(|v| v["source"].as_str());
            let resource = source.and_then(|source| resources.borrow().iter().find(|r| r.uri().as_deref() == Some(source)).cloned());
            data.as_object_mut().unwrap().remove("selected");
            data["selected"] = serde_json::json!({"found":source.is_some(), "blob":source.is_some_and(|s|s.starts_with("blob:")), "canvasReadable":selected.as_ref().is_some_and(|v|v["png"].is_string()), "nativeResourceFound":resource.is_some(), "nativeImageMime":resource.as_ref().and_then(|r|r.response()).and_then(|r|r.mime_type()).map(|m|m.starts_with("image/")), "resourceBytes":resource.as_ref().and_then(|r|r.response()).map(|r|r.content_length())});
            let url = source.and_then(|s| tauri::Url::parse(s).ok());
            data["selected"]["legacyHostAllowed"] = serde_json::json!(url.as_ref().is_some_and(|u| u.scheme() == "https" && matches!(u.host_str(), Some("pp.whatsapp.net" | "pps.whatsapp.net"))));
            data["selected"]["secondsSinceOpening"] = serde_json::json!(started.elapsed().as_secs());
            let mime = resource.as_ref().and_then(|r|r.response()).and_then(|r|r.mime_type());
            data["selected"]["format"] = serde_json::json!(match mime.as_deref() { Some("image/png") => "png", Some("image/jpeg") => "jpeg", Some("image/webp") => "webp", Some("image/avif") => "avif", _ => "other" });
            data["conversion"] = converted.borrow().1.clone();
            if let (Some(source), Some(resource)) = (source, resource) {
                let source = source.to_owned();
                if converted.borrow().0 != source && resource.response().is_some_and(|r|r.content_length() > 0 && r.content_length() <= 256*1024) {
                    converted.borrow_mut().0 = source;
                    WebResourceExt::data(&resource, None::<&gtk::gio::Cancellable>, move |result| {
                        let mut status = serde_json::json!({"readable":false,"normalized":false});
                        if let Ok(bytes) = result {
                            status["readable"] = true.into();
                            let format = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") { Some("png") } else if bytes.starts_with(&[0xff,0xd8,0xff]) { Some("jpeg") } else { None };
                            status["recognizedRaster"] = serde_json::json!(format);
                            if let Some(loader) = format.and_then(|f|gtk::gdk_pixbuf::PixbufLoader::with_type(f).ok()) {
                                loader.set_size(64,64);
                                let written = loader.write(&bytes);
                                let closed = loader.close();
                                if written.is_ok() && closed.is_ok() {
                                    if let Some(image) = loader.pixbuf() {
                                        status["width"] = image.width().into();
                                        status["height"] = image.height().into();
                                        status["normalized"] = (image.width()==64 && image.height()==64).into();
                                        preview.set_from_pixbuf(Some(&image));
                                    }
                                }
                            }
                        }
                        converted.borrow_mut().1 = status;
                    });
                }
            }
            let _ = std::fs::write(report, serde_json::to_vec_pretty(&data).unwrap());
        });
        glib::ControlFlow::Continue
    });
    view.load_uri("https://web.whatsapp.com/");
    window.show_all();
    gtk::main();
}

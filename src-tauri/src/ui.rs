use crate::{
    controller::{self, Request},
    storage, ui_layout,
};
use anyhow::Result;
use gtk::prelude::*;
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{webview::WebviewBuilder, Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl};
use webkit2gtk::{NotificationExt, PermissionRequestExt, SettingsExt, WebContextExt, WebViewExt};

struct UiState {
    runtime: PathBuf,
    profile: PathBuf,
    selected: Mutex<Option<String>>,
}
fn local(webview: &tauri::Webview) -> Result<(), String> {
    if webview.label() != "main" {
        return Err("Unauthorized origin".into());
    }
    let u = webview.url().map_err(|_| "Invalid origin")?;
    if u.scheme() != "tauri" && !(u.scheme() == "http" && u.host_str() == Some("tauri.localhost")) {
        return Err("Unauthorized origin".into());
    }
    Ok(())
}
#[tauri::command]
async fn request(
    webview: tauri::Webview,
    state: tauri::State<'_, UiState>,
    op: String,
    data: Value,
) -> Result<Value, String> {
    local(&webview)?;
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || controller::rpc(&runtime, Request { op, data }))
        .await
        .map_err(|_| "Operation failed".to_string())?
        .map_err(|e| e.to_string())
}
#[tauri::command]
fn select_account(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    state: tauri::State<'_, UiState>,
    id: Option<String>,
) -> Result<(), String> {
    local(&webview)?;
    if let Some(id) = &id {
        if app.get_webview(&format!("account-{id}")).is_none() {
            return Err("Account not found".into());
        }
    }
    for (label, v) in app.webviews() {
        if label.starts_with("account-") {
            if id
                .as_ref()
                .is_some_and(|id| label == format!("account-{id}"))
            {
                v.show().map_err(|e| e.to_string())?;
                v.set_focus().map_err(|e| e.to_string())?;
            } else {
                v.hide().map_err(|e| e.to_string())?;
            }
        }
    }
    *state.selected.lock().unwrap() = id;
    Ok(())
}
#[tauri::command]
fn sync_accounts(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    state: tauri::State<'_, UiState>,
) -> Result<(), String> {
    local(&webview)?;
    let status = controller::rpc(
        &state.runtime,
        Request {
            op: "status".into(),
            data: Value::Null,
        },
    )
    .map_err(|e| e.to_string())?;
    if status["unlocked"] != true {
        return Ok(());
    }
    for a in status["accounts"].as_array().ok_or("Invalid accounts")? {
        let id = a["id"].as_str().ok_or("Invalid account")?;
        uuid::Uuid::parse_str(id).map_err(|_| "Invalid account")?;
        let label = format!("account-{id}");
        if app.get_webview(&label).is_some() {
            continue;
        }
        let p = state.profile.join("accounts").join(id);
        storage::private_dir(&p).map_err(|e| e.to_string())?;
        let runtime = state.runtime.clone();
        let h = app.clone();
        let account_id = id.to_string();
        let builder =
            WebviewBuilder::new(&label, WebviewUrl::External("about:blank".parse().unwrap()))
                .data_directory(p)
                .devtools(false)
                .on_download(|_, event| match event {
                    tauri::webview::DownloadEvent::Requested { destination, .. } => {
                        let dialog = gtk::FileChooserDialog::new(
                            Some("Export file — the copy will be outside the vault"),
                            None::<&gtk::Window>,
                            gtk::FileChooserAction::Save,
                        );
                        dialog.add_buttons(&[
                            ("Cancel", gtk::ResponseType::Cancel),
                            ("Export", gtk::ResponseType::Accept),
                        ]);
                        dialog.set_do_overwrite_confirmation(true);
                        if let Some(name) = destination.file_name().and_then(|n| n.to_str()) {
                            dialog.set_current_name(name);
                        }
                        let accepted = dialog.run() == gtk::ResponseType::Accept;
                        let path = dialog.filename();
                        dialog.close();
                        if accepted {
                            if let Some(path) = path {
                                *destination = path;
                                return true;
                            }
                        }
                        false
                    }
                    tauri::webview::DownloadEvent::Finished { .. } => true,
                    _ => false,
                })
                .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
                .on_navigation(|url| {
                    url.as_str() == "about:blank"
                        || (url.scheme() == "https" && url.host_str() == Some("web.whatsapp.com"))
                });
        let window = app.get_window("main").ok_or("Window unavailable")?;
        let size = window
            .inner_size()
            .map_err(|e| e.to_string())?
            .to_logical::<f64>(window.scale_factor().map_err(|e| e.to_string())?);
        let view = window
            .add_child(
                builder,
                LogicalPosition::new(304., 72.),
                LogicalSize::new((size.width - 304.).max(1.), (size.height - 72.).max(1.)),
            )
            .map_err(|e| e.to_string())?;
        view.hide().map_err(|e| e.to_string())?;
        view.with_webview(move |platform| {
            let w = platform.inner();
            if let Err(error) = ui_layout::attach(&w, true) {
                let _ = h.emit_to("main", "account-error", error.to_string());
                return;
            }
            if !w.context().is_some_and(|c| c.is_sandbox_enabled()) {
                let _ = h.emit_to(
                    "main",
                    "account-error",
                    "The WebKit sandbox is unavailable. This account was not loaded.",
                );
                return;
            }
            if let Some(settings) = WebViewExt::settings(&w) {
                settings.set_enable_developer_extras(false);
                settings.set_enable_page_cache(false);
            }
            w.connect_create(|_, _| None);
            w.connect_permission_request(|w, req| {
                let allowed = w
                    .uri()
                    .is_some_and(|s| s.starts_with("https://web.whatsapp.com/"))
                    && req.is::<webkit2gtk::NotificationPermissionRequest>();
                if allowed {
                    req.allow();
                } else {
                    req.deny();
                }
                true
            });
            let h2 = h.clone();
            let id2 = account_id.clone();
            w.connect_show_notification(move |_, note| {
                // Replace server-provided title/body before anything reaches the desktop daemon.
                let h = h2.clone();
                let id = id2.clone();
                std::thread::spawn(move || {
                    if let Ok(n) = notify_rust::Notification::new()
                        .appname("Elo Talks")
                        .summary("Elo Talks")
                        .body("You have a new message.")
                        .action("default", "Open account")
                        .show()
                    {
                        n.wait_for_action(move |action| {
                            if action == "default" {
                                let h2 = h.clone();
                                let id2 = id.clone();
                                let _ = h.run_on_main_thread(move || {
                                    if let Some(w) = h2.get_window("main") {
                                        let _ = w.show();
                                        let _ = w.set_focus();
                                    }
                                    let _ = h2.emit_to("main", "open-account", id2);
                                });
                            }
                        });
                    }
                });
                note.close();
                true
            });
            let last = std::rc::Rc::new(std::cell::RefCell::new(Instant::now()));
            w.connect_event(move |_, event| {
                if !matches!(
                    event.event_type(),
                    gtk::gdk::EventType::KeyPress
                        | gtk::gdk::EventType::ButtonPress
                        | gtk::gdk::EventType::Scroll
                        | gtk::gdk::EventType::MotionNotify
                ) {
                    return gtk::glib::Propagation::Proceed;
                }
                if last.borrow().elapsed() > Duration::from_secs(1) {
                    *last.borrow_mut() = Instant::now();
                    let _ = controller::rpc(
                        &runtime,
                        Request {
                            op: "activity".into(),
                            data: Value::Null,
                        },
                    );
                }
                gtk::glib::Propagation::Proceed
            });
            w.load_uri("https://web.whatsapp.com/");
        })
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}
pub fn run(runtime: PathBuf, profile: PathBuf) -> Result<()> {
    let rt = runtime.clone();
    tauri::Builder::default()
        .manage(UiState {
            runtime,
            profile: profile.clone(),
            selected: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            request,
            select_account,
            sync_accounts
        ])
        .setup(move |app| {
            let window = tauri::window::WindowBuilder::new(app, "main")
                .title("Elo Talks")
                .inner_size(1200., 800.)
                .min_inner_size(900., 640.)
                .build()?;
            let local = WebviewBuilder::new("main", WebviewUrl::App("index.html".into()))
                .data_directory(profile.join("shell"))
                .devtools(false)
                .on_navigation(|url| {
                    url.scheme() == "tauri"
                        || (url.scheme() == "http" && url.host_str() == Some("tauri.localhost"))
                });
            let shell = window.add_child(
                local,
                LogicalPosition::new(0., 0.),
                LogicalSize::new(1200., 800.),
            )?;
            shell.with_webview(|platform| {
                if let Err(error) = ui_layout::attach(&platform.inner(), false) {
                    eprintln!("Elo Talks: {error}");
                    // Fail closed instead of running accounts without trusted controls.
                    std::process::exit(1);
                }
            })?;
            let open =
                tauri::menu::MenuItem::with_id(app, "open", "Open Elo Talks", true, None::<&str>)?;
            let lock =
                tauri::menu::MenuItem::with_id(app, "lock", "Lock vault", true, None::<&str>)?;
            let quit = tauri::menu::MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = tauri::menu::Menu::with_items(app, &[&open, &lock, &quit])?;
            let rt2 = rt.clone();
            let tray = tauri::tray::TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("Elo Talks")
                .menu(&menu)
                .on_menu_event(move |app, event| match event.id().as_ref() {
                    "open" => {
                        if let Some(w) = app.get_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "lock" | "quit" => {
                        let rt = rt2.clone();
                        let op = event.id().as_ref().to_string();
                        std::thread::spawn(move || {
                            let _ = controller::rpc(
                                &rt,
                                Request {
                                    op,
                                    data: Value::Null,
                                },
                            );
                        });
                    }
                    _ => {}
                })
                .build(app);
            let tray_ok = tray.is_ok()
                && dbus::blocking::Connection::new_session()
                    .ok()
                    .and_then(|c| {
                        c.with_proxy(
                            "org.freedesktop.DBus",
                            "/org/freedesktop/DBus",
                            Duration::from_secs(1),
                        )
                        .method_call::<(bool,), _, _, _>(
                            "org.freedesktop.DBus",
                            "NameHasOwner",
                            ("org.kde.StatusNotifierWatcher",),
                        )
                        .ok()
                        .map(|v| v.0)
                    })
                    .unwrap_or(false);
            let handle = app.handle().clone();
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    if tray_ok {
                        if let Some(w) = handle.get_window("main") {
                            let _ = w.hide();
                        }
                    }
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())?;
    Ok(())
}

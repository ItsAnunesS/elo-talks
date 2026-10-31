use crate::{
    account_events::{self, AccountEvents},
    account_runtime::{AccountHealth, NotificationPreferences, PageState},
    ipc::{self, Request},
    notification_content::NotificationText,
    storage, ui_layout,
};
use anyhow::Result;
use gtk::prelude::*;
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};
use tauri::{webview::WebviewBuilder, Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl};
use webkit2gtk::{SettingsExt, WebContextExt, WebViewExt};

struct UiState {
    runtime: PathBuf,
    profile: PathBuf,
    activity: mpsc::SyncSender<()>,
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
fn window_action(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    action: String,
) -> Result<(), String> {
    local(&webview)?;
    let window = app.get_window("main").ok_or("Window unavailable")?;
    match action.as_str() {
        "drag" => window.start_dragging(),
        "minimize" => window.minimize(),
        "maximize" => {
            if window.is_maximized().map_err(|error| error.to_string())? {
                window.unmaximize()
            } else {
                window.maximize()
            }
        }
        "close" => window.close(),
        _ => return Err("Unsupported window action".into()),
    }
    .map_err(|error| error.to_string())
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
    tauri::async_runtime::spawn_blocking(move || ipc::rpc(&runtime, Request { op, data }))
        .await
        .map_err(|_| "Operation failed".to_string())?
        .map_err(|e| e.to_string())
}
#[tauri::command]
fn select_account(
    webview: tauri::Webview,
    app: tauri::AppHandle,
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
            let visible = id
                .as_ref()
                .is_some_and(|id| label == format!("account-{id}"));
            ui_layout::set_account_visible(&v, visible).map_err(|e| e.to_string())?;
        }
    }
    {
        let events = app.state::<AccountEvents>();
        let mut runtime = events.runtime.lock().unwrap();
        runtime.selected = id;
        runtime.acknowledge_selected();
    }
    account_events::publish(&app);
    Ok(())
}
#[tauri::command]
fn account_snapshot(webview: tauri::Webview, app: tauri::AppHandle) -> Result<Value, String> {
    local(&webview)?;
    serde_json::to_value(&*app.state::<AccountEvents>().runtime.lock().unwrap())
        .map_err(|error| error.to_string())
}
#[tauri::command]
fn account_avatar(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    id: String,
) -> Result<Option<String>, String> {
    local(&webview)?;
    Ok(crate::account_avatar::current(&app, &id))
}

#[tauri::command]
fn retry_account(webview: tauri::Webview, app: tauri::AppHandle, id: String) -> Result<(), String> {
    local(&webview)?;
    let view = app
        .get_webview(&format!("account-{id}"))
        .ok_or("Account not found")?;
    view.with_webview(move |platform| {
        let view = platform.inner();
        if view
            .context()
            .is_some_and(|context| context.is_sandbox_enabled())
        {
            view.load_uri("https://web.whatsapp.com/");
        }
    })
    .map_err(|error| error.to_string())
}
#[tauri::command]
fn diagnostics(webview: tauri::Webview, app: tauri::AppHandle) -> Result<Value, String> {
    local(&webview)?;
    let events = app.state::<AccountEvents>();
    let runtime = events.runtime.lock().unwrap();
    Ok(runtime.diagnostics())
}
#[tauri::command]
async fn test_notification(webview: tauri::Webview, app: tauri::AppHandle) -> Result<(), String> {
    local(&webview)?;
    let (sender, receiver) = mpsc::sync_channel(1);
    let handle = app.clone();
    app.run_on_main_thread(move || {
        gtk::glib::MainContext::default().spawn_local(async move {
            let result = account_events::send_desktop_notification(
                &handle,
                "test",
                NotificationText::test(),
            )
            .await;
            let result = match result {
                Ok(true) => Ok(()),
                Ok(false) => {
                    Err("A notification test is already pending. Try again shortly.".to_string())
                }
                Err(error) => Err(error.to_string()),
            };
            let _ = sender.send(result);
        });
    })
    .map_err(|_| "Could not start the notification test".to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        receiver
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| "The Linux notification service did not respond".to_string())?
    })
    .await
    .map_err(|_| "The notification test failed".to_string())?
}

#[tauri::command]
fn sync_accounts(
    account_id: Option<String>,
    webview: tauri::Webview,
    app: tauri::AppHandle,
    state: tauri::State<'_, UiState>,
) -> Result<(), String> {
    local(&webview)?;
    let status = ipc::rpc(
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
    {
        let events = app.state::<AccountEvents>();
        let mut runtime = events.runtime.lock().unwrap();
        runtime.preferences =
            serde_json::from_value::<NotificationPreferences>(status["notifications"].clone())
                .map_err(|error| error.to_string())?;
    }
    for a in status["accounts"].as_array().ok_or("Invalid accounts")? {
        let id = a["id"].as_str().ok_or("Invalid account")?;
        if account_id.as_ref().is_some_and(|requested| requested != id) {
            continue;
        }
        uuid::Uuid::parse_str(id).map_err(|_| "Invalid account")?;
        // Refresh labels even when an existing channel is renamed without reloading it.
        app.state::<AccountEvents>()
            .runtime
            .lock()
            .unwrap()
            .channel_names
            .insert(
                id.to_owned(),
                a["name"].as_str().ok_or("Invalid channel name")?.to_owned(),
            );
        let label = format!("account-{id}");
        if app.get_webview(&label).is_some() {
            continue;
        }
        app.state::<AccountEvents>()
            .runtime
            .lock()
            .unwrap()
            .accounts
            .insert(id.to_owned(), AccountHealth::default());
        let p = state.profile.join("accounts").join(id);
        storage::private_dir(&p).map_err(|e| e.to_string())?;
        crate::account_avatar::restore(&app, id, p.join("self-avatar.json"));
        let activity = state.activity.clone();
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
                    url.as_str() == "about:blank" || crate::whatsapp::is_origin(url)
                });
        let window = app.get_window("main").ok_or("Window unavailable")?;
        let size = window
            .inner_size()
            .map_err(|e| e.to_string())?
            .to_logical::<f64>(window.scale_factor().map_err(|e| e.to_string())?);
        let view = window
            .add_child(
                builder,
                LogicalPosition::new(
                    ui_layout::SIDEBAR_WIDTH as f64,
                    ui_layout::HEADER_HEIGHT as f64,
                ),
                LogicalSize::new(
                    (size.width - ui_layout::SIDEBAR_WIDTH as f64).max(1.),
                    (size.height - ui_layout::HEADER_HEIGHT as f64).max(1.),
                ),
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
                account_events::page(&h, &account_id, PageState::SandboxUnavailable);
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
            account_events::attach(&w, &h, &account_id);
            crate::account_readiness::attach(&w, &h, &account_id);
            crate::account_avatar::attach(&w, &h, &account_id);
            w.connect_create(|_, _| None);
            crate::account_permissions::attach(&w);
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
                    // Coalesce pending activity updates without blocking GTK input handling.
                    let _ = activity.try_send(());
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
    let (activity, activity_events) = mpsc::sync_channel(1);
    let activity_runtime = runtime.clone();
    std::thread::spawn(move || {
        while activity_events.recv().is_ok() {
            let _ = ipc::rpc(
                &activity_runtime,
                Request {
                    op: "activity".into(),
                    data: Value::Null,
                },
            );
        }
    });
    tauri::Builder::default()
        .manage(AccountEvents::default())
        .manage(UiState {
            runtime,
            profile: profile.clone(),
            activity,
        })
        .invoke_handler(tauri::generate_handler![
            request,
            window_action,
            select_account,
            sync_accounts,
            account_snapshot,
            account_avatar,
            retry_account,
            diagnostics,
            test_notification
        ])
        .setup(move |app| {
            let window = tauri::window::WindowBuilder::new(app, "main")
                .title("Elo Talks")
                .decorations(false)
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
            let tray = tauri::tray::TrayIconBuilder::with_id("main-tray")
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
                            let _ = ipc::rpc(
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
            app.state::<AccountEvents>().runtime.lock().unwrap().focused =
                window.is_focused().unwrap_or(false);
            account_events::publish(app.handle());
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::Focused(focused) = event {
                    {
                        let events = handle.state::<AccountEvents>();
                        let mut runtime = events.runtime.lock().unwrap();
                        runtime.focused = *focused;
                        runtime.acknowledge_selected();
                    }
                    account_events::publish(&handle);
                }
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

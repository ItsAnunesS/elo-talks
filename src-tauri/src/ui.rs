use crate::{
    account_events::{self, AccountEvents},
    account_runtime::{AccountHealth, NotificationPreferences, PageState},
    ipc::{self, Request},
    notification_content::NotificationText,
    ui_layout,
};
use anyhow::Result;
use gtk::prelude::*;
use serde_json::Value;
use std::{path::PathBuf, sync::mpsc, time::Duration};
use tauri::{webview::WebviewBuilder, LogicalPosition, LogicalSize, Manager, WebviewUrl};
use tauri_runtime_cef::{
    Cef, DebugEnvironment, DevToolsPolicy, FrameEventKind, RemoteDebugging, SandboxPolicy,
    WebviewBuilderCefExt,
};

type AccountPreparation = dyn Fn(&tauri::Webview) -> tauri::Result<()> + Send + Sync;

struct UiState {
    runtime: PathBuf,
    profile: PathBuf,
    activity: mpsc::SyncSender<()>,
    prepare_account: std::sync::Arc<AccountPreparation>,
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
        "resize-n" => window.start_resize_dragging(tauri_runtime::ResizeDirection::North),
        "resize-ne" => window.start_resize_dragging(tauri_runtime::ResizeDirection::NorthEast),
        "resize-e" => window.start_resize_dragging(tauri_runtime::ResizeDirection::East),
        "resize-se" => window.start_resize_dragging(tauri_runtime::ResizeDirection::SouthEast),
        "resize-s" => window.start_resize_dragging(tauri_runtime::ResizeDirection::South),
        "resize-sw" => window.start_resize_dragging(tauri_runtime::ResizeDirection::SouthWest),
        "resize-w" => window.start_resize_dragging(tauri_runtime::ResizeDirection::West),
        "resize-nw" => window.start_resize_dragging(tauri_runtime::ResizeDirection::NorthWest),
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
async fn choose_backup(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    export: bool,
) -> Result<Option<String>, String> {
    local(&webview)?;
    let (sender, receiver) = mpsc::sync_channel(1);
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let parent = handle
            .get_window("main")
            .and_then(|window| window.gtk_window().ok());
        let dialog = gtk::FileDialog::builder()
            .title(if export {
                "Export encrypted backup"
            } else {
                "Import encrypted backup"
            })
            .modal(true)
            .build();
        let filter = gtk::FileFilter::new();
        filter.set_name(Some("Elo Talks encrypted backup"));
        filter.add_pattern("*.elobackup");
        let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        dialog.set_filters(Some(&filters));
        if export {
            dialog.set_initial_name(Some(&format!(
                "elo-talks-{}.elobackup",
                uuid::Uuid::new_v4()
            )));
        }
        gtk::glib::MainContext::default().spawn_local(async move {
            let result = if export {
                dialog.save_future(parent.as_ref()).await
            } else {
                dialog.open_future(parent.as_ref()).await
            };
            let path = match result {
                Ok(file) => file
                    .path()
                    .map(|path| Some(path.to_string_lossy().into_owned()))
                    .ok_or_else(|| "Choose a local backup file".to_string()),
                Err(error)
                    if error.matches(gtk::DialogError::Dismissed)
                        || error.matches(gtk::DialogError::Cancelled) =>
                {
                    Ok(None)
                }
                Err(_) => Err(
                    "The file chooser could not open. Check your desktop file portal.".to_string(),
                ),
            };
            let _ = sender.send(path);
        });
    })
    .map_err(|_| "Could not open the file chooser".to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        receiver
            .recv()
            .map_err(|_| "File chooser closed".to_string())?
    })
    .await
    .map_err(|_| "File chooser failed".to_string())?
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
    // Reveal the destination before hiding its siblings so the GTK frame never
    // exposes the empty shell between already loaded accounts.
    if let Some(id) = &id {
        if let Some(view) = app.get_webview(&format!("account-{id}")) {
            ui_layout::set_account_visible(&view, true).map_err(|error| error.to_string())?;
        }
    }
    for (label, view) in app.webviews() {
        if label.starts_with("account-")
            && !id
                .as_ref()
                .is_some_and(|id| label == format!("account-{id}"))
        {
            ui_layout::set_account_visible(&view, false).map_err(|error| error.to_string())?;
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
    view.navigate("https://web.whatsapp.com/".parse().unwrap())
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
        let p = crate::account_profiles::account(&state.profile, id).map_err(|e| e.to_string())?;
        crate::account_avatar::restore(&app, id, p.join("self-avatar.json"));
        let h = app.clone();
        let title_handle = app.clone();
        let title_id = id.to_owned();
        let page_handle = app.clone();
        let page_id = id.to_owned();
        let account_id = id.to_string();
        // CEF callbacks can run outside a winit dispatch. Never call synchronous
        // webview getters there: they would wait for the same UI thread.
        let trusted_origin = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let permission_origin = trusted_origin.clone();
        let title_origin = trusted_origin.clone();
        let builder =
            WebviewBuilder::new(&label, WebviewUrl::External("about:blank".parse().unwrap()))
                .data_directory(p)
                .devtools(false)
                .on_permission_request(move |_, kind| match kind {
                    tauri::webview::PermissionKind::Camera
                    | tauri::webview::PermissionKind::Microphone
                        if permission_origin.load(std::sync::atomic::Ordering::Relaxed) =>
                    {
                        tauri::webview::PermissionResponse::Default
                    }
                    _ => tauri::webview::PermissionResponse::Deny,
                })
                .on_document_title_changed(move |_, title| {
                    let count = crate::whatsapp::unread_count(
                        title_origin
                            .load(std::sync::atomic::Ordering::Relaxed)
                            .then_some("https://web.whatsapp.com/"),
                        Some(&title),
                    );
                    if title_handle
                        .state::<AccountEvents>()
                        .runtime
                        .lock()
                        .unwrap()
                        .update_unread(&title_id, count)
                    {
                        account_events::publish(&title_handle);
                    }
                })
                .on_frame_event(move |event| {
                    if event.is_main {
                        let trusted = match &event.kind {
                            FrameEventKind::DocumentCommitted { url }
                            | FrameEventKind::AddressChanged { url } => {
                                Some(crate::whatsapp::is_origin(url))
                            }
                            FrameEventKind::NavigationStarted { .. }
                            | FrameEventKind::NavigationFailed { .. }
                            | FrameEventKind::Destroyed => Some(false),
                            _ => None,
                        };
                        if let Some(trusted) = trusted {
                            trusted_origin.store(trusted, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                    if matches!(event.kind, FrameEventKind::RendererTerminated) {
                        trusted_origin.store(false, std::sync::atomic::Ordering::Relaxed);
                    }
                    let state = match event.kind {
                        FrameEventKind::NavigationStarted { .. } if event.is_main => {
                            Some(PageState::Loading)
                        }
                        FrameEventKind::NavigationFailed { .. } if event.is_main => {
                            Some(PageState::LoadFailed)
                        }
                        FrameEventKind::RendererTerminated => Some(PageState::ProcessStopped),
                        _ => None,
                    };
                    if let Some(state) = state {
                        account_events::page(&page_handle, &page_id, state);
                    }
                })
                .on_page_load(move |view, event| {
                    if event.event() == tauri::webview::PageLoadEvent::Finished
                        && crate::whatsapp::is_origin(event.url())
                    {
                        let id = view.label().strip_prefix("account-").unwrap_or_default();
                        account_events::page(view.app_handle(), id, PageState::Ready);
                        crate::account_readiness::attach(&view, view.app_handle(), id);
                        crate::account_avatar::attach(&view, view.app_handle(), id);
                        crate::whatsapp_style::attach(&view);
                    }
                })
                .on_download(|_, event| match event {
                    tauri::webview::DownloadEvent::Requested { destination, .. } => {
                        let dialog = gtk::FileDialog::builder()
                            .title("Export file — the copy will be outside the vault")
                            .modal(true)
                            .build();
                        if let Some(name) = destination.file_name().and_then(|n| n.to_str()) {
                            dialog.set_initial_name(Some(name));
                        }
                        if let Ok(file) = gtk::glib::MainContext::default()
                            .block_on(dialog.save_future(None::<&gtk::Window>))
                        {
                            if let Some(path) = file.path() {
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
                builder
                    .position(LogicalPosition::new(
                        ui_layout::SIDEBAR_WIDTH as f64,
                        ui_layout::HEADER_HEIGHT as f64,
                    ))
                    .size(LogicalSize::new(
                        (size.width - ui_layout::SIDEBAR_WIDTH as f64 - ui_layout::RESIZE_BORDER)
                            .max(1.),
                        (size.height - ui_layout::HEADER_HEIGHT as f64 - ui_layout::RESIZE_BORDER)
                            .max(1.),
                    )),
            )
            .map_err(|e| e.to_string())?;
        view.hide().map_err(|e| e.to_string())?;
        (state.prepare_account)(&view).map_err(|error| error.to_string())?;
        crate::account_avatar::observe(&view, &h, &account_id).map_err(|e| e.to_string())?;
        account_events::attach(&view, &h, &account_id, state.activity.clone())
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
pub fn run(runtime: PathBuf, profile: PathBuf) -> Result<()> {
    run_with_setup(runtime, profile, |_| Ok(()))
}

pub fn run_with_setup(
    runtime: PathBuf,
    profile: PathBuf,
    ready: impl FnOnce(&mut tauri::App) -> Result<()> + Send + 'static,
) -> Result<()> {
    run_with_account_setup(runtime, profile, ready, |_| Ok(()))
}

pub fn run_with_account_setup(
    runtime: PathBuf,
    profile: PathBuf,
    ready: impl FnOnce(&mut tauri::App) -> Result<()> + Send + 'static,
    prepare_account: impl Fn(&tauri::Webview) -> tauri::Result<()> + Send + Sync + 'static,
) -> Result<()> {
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
    let chromium_root = crate::account_profiles::root(&profile)?;
    let cef = Cef::default()
        .sandbox(SandboxPolicy::Required)
        .certificate_errors(tauri_runtime_cef::CertificateErrorPolicy::Cancel)
        .root_cache_path(&chromium_root)
        .with_settings(move |settings| {
            // Bootstrap without opening persistent browser data. Each account
            // receives its own context only after protect_secrets below.
            settings.root_cache_path = chromium_root.to_string_lossy().as_ref().into();
            settings.cache_path = Default::default();
        })
        .debug_environment(DebugEnvironment::Deny)
        .log_severity(tauri_runtime_cef::cef::LogSeverity::DISABLE)
        .remote_debugging(RemoteDebugging::Disabled)
        .devtools(DevToolsPolicy::Disallowed)
        .allow_chromium_command_line_args(false)
        .default_content_setting(
            tauri_runtime_cef::cef::ContentSettingTypes::NOTIFICATIONS,
            tauri_runtime_cef::cef::ContentSettingValues::BLOCK,
        );
    tauri::Builder::default()
        .runtime(cef)
        .manage(AccountEvents::default())
        .manage(UiState {
            runtime,
            profile: profile.clone(),
            activity,
            prepare_account: std::sync::Arc::new(prepare_account),
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
            test_notification,
            choose_backup
        ])
        .setup(move |app| {
            crate::process_security::protect_secrets()?;
            let shutdown = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            signal_hook::flag::register(libc::SIGTERM, shutdown.clone())?;
            let exit_handle = app.handle().clone();
            gtk::glib::timeout_add_local(Duration::from_millis(50), move || {
                if shutdown.load(std::sync::atomic::Ordering::Relaxed) {
                    for window in exit_handle.windows().values() {
                        let _ = window.hide();
                    }
                    exit_handle.exit(0);
                    gtk::glib::ControlFlow::Break
                } else {
                    gtk::glib::ControlFlow::Continue
                }
            });
            let geometry = crate::window_state::load(&profile);
            let window = tauri::window::WindowBuilder::new(app, "main")
                .title("Elo Talks")
                .decorations(false)
                .resizable(true)
                .inner_size(geometry.width as f64, geometry.height as f64)
                .maximized(geometry.maximized)
                .min_inner_size(900., 640.)
                .build()?;
            crate::window_state::attach(&window, profile.join("window.json"), geometry);
            let local = WebviewBuilder::new("main", WebviewUrl::App("index.html".into()))
                .data_directory(profile.join("chromium").join("shell"))
                .devtools(false)
                .on_navigation(|url| {
                    url.scheme() == "tauri"
                        || (url.scheme() == "http" && url.host_str() == Some("tauri.localhost"))
                });
            let shell = window.add_child(
                local
                    .position(LogicalPosition::new(0., 0.))
                    .size(LogicalSize::new(1200., 800.)),
            )?;
            ui_layout::attach(&window, &shell)?;
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
            ready(app)?;
            Ok(())
        })
        .run(tauri::generate_context!())?;
    Ok(())
}

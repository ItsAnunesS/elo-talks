//! Native adapters for page health, notification delivery and desktop counters.
use crate::{
    account_runtime::{AccountRuntime, NotificationDelivery, PageState},
    desktop_notifications::DesktopNotifications,
    notification_content::{NotificationDetail, NotificationText},
};
use dbus::{
    arg::{PropMap, Variant},
    channel::Sender,
};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    mpsc, Arc, Mutex,
};
use tauri::{Emitter, Manager};

// AppImage managers may rename the desktop entry; include the launched identity.
fn desktop_ids() -> Vec<String> {
    let mut ids = vec![
        "dev.anuness.elotalks.desktop".into(),
        "Elo Talks.desktop".into(),
        "elo_talks.desktop".into(),
    ];
    if let Some(path) = std::env::var_os("GIO_LAUNCHED_DESKTOP_FILE") {
        if let Some(name) = std::path::Path::new(&path)
            .file_name()
            .and_then(|name| name.to_str())
        {
            if name.ends_with(".desktop") && !ids.iter().any(|id| id == name) {
                ids.push(name.to_owned());
            }
        }
    }
    ids
}

fn send_badge(connection: &dbus::blocking::Connection, total: u64) {
    for id in desktop_ids() {
        let mut properties = PropMap::new();
        properties.insert("count".into(), Variant(Box::new(total as i64)));
        properties.insert("count-visible".into(), Variant(Box::new(total > 0)));
        if let Ok(message) = dbus::Message::new_signal(
            "/dev/anuness/elotalks",
            "com.canonical.Unity.LauncherEntry",
            "Update",
        ) {
            let _ = connection.send(message.append2(format!("application://{id}"), properties));
        }
    }
}

pub fn clear_desktop_badge() {
    if let Ok(connection) = dbus::blocking::Connection::new_session() {
        send_badge(&connection, 0);
    }
}

pub struct AccountEvents {
    pub scripts: Mutex<std::collections::BTreeMap<String, Arc<crate::account_script::Script>>>,
    pub runtime: Mutex<AccountRuntime>,
    pub avatars: Mutex<std::collections::BTreeMap<String, crate::account_avatar::Avatar>>,
    pub avatar_cache_paths: Mutex<std::collections::BTreeMap<String, std::path::PathBuf>>,
    pub avatar_resources:
        Mutex<std::collections::BTreeMap<String, Arc<crate::account_avatar::Resources>>>,
    badge_count: Arc<AtomicU64>,
    badge_wake: mpsc::SyncSender<()>,
    desktop: DesktopNotifications,
}

impl Default for AccountEvents {
    fn default() -> Self {
        let badge_count = Arc::new(AtomicU64::new(u64::MAX));
        let count = badge_count.clone();
        let (badge_wake, receiver) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let connection = dbus::blocking::Connection::new_session().ok();
            while receiver.recv().is_ok() {
                if let Some(connection) = &connection {
                    let total = count.load(Ordering::Relaxed);
                    send_badge(connection, total);
                }
            }
        });
        Self {
            runtime: Mutex::new(AccountRuntime::default()),
            scripts: Mutex::default(),
            avatars: Mutex::default(),
            avatar_cache_paths: Mutex::default(),
            avatar_resources: Mutex::default(),
            badge_count,
            badge_wake,
            desktop: DesktopNotifications::default(),
        }
    }
}

pub fn publish(app: &tauri::AppHandle) {
    let events = app.state::<AccountEvents>();
    let (total, snapshot) = {
        let runtime = events.runtime.lock().unwrap();
        (
            runtime.total(),
            serde_json::to_value(&*runtime).expect("Account state serialization"),
        )
    };
    let _ = app.emit_to("main", "account-state", snapshot);
    // Page-state events do not need to rewrite the title or send duplicate dock updates.
    if events.badge_count.swap(total, Ordering::Relaxed) == total {
        return;
    }
    let _ = events.badge_wake.try_send(());
    if let Some(window) = app.get_window("main") {
        let title = if total == 0 {
            "Elo Talks".into()
        } else {
            format!("Elo Talks ({total})")
        };
        let _ = window.set_title(&title);
    }
    if let Some(tray) = app.tray_by_id("main-tray") {
        let _ = tray.set_tooltip(Some(format!(
            "Elo Talks · {total} unread items / pending notifications"
        )));
    }
}

pub fn page(app: &tauri::AppHandle, id: &str, page: PageState) {
    app.state::<AccountEvents>()
        .runtime
        .lock()
        .unwrap()
        .update_page(id, page);
    publish(app);
}

pub fn attach(
    view: &tauri::Webview,
    app: &tauri::AppHandle,
    id: &str,
    activity: mpsc::SyncSender<()>,
) -> tauri::Result<()> {
    app.state::<AccountEvents>()
        .scripts
        .lock()
        .unwrap()
        .insert(id.to_owned(), crate::account_script::Script::attach(view)?);
    use tauri_runtime_cef::{DevToolsProtocol, WebviewCefExt};
    let tls_handle = app.clone();
    let tls_account = id.to_owned();
    view.on_dev_tools_protocol(move |event| {
        if let DevToolsProtocol::Event { method, params } = event {
            if method != "Network.loadingFailed" || params.len() > 4096 {
                return;
            }
            let Ok(value) = serde_json::from_slice::<serde_json::Value>(&params) else {
                return;
            };
            if value["type"] == "Document"
                && value["errorText"].as_str().is_some_and(|error| {
                    error.starts_with("net::ERR_CERT_") || error.starts_with("net::ERR_SSL_")
                })
            {
                page(&tls_handle, &tls_account, PageState::TlsFailed);
            }
        }
    })?;
    let handle = app.clone();
    let account = id.to_owned();
    let failure_handle = app.clone();
    let failure_id = id.to_owned();
    crate::cef_notifications::attach(
        view,
        "https://web.whatsapp.com",
        move |title, body| {
            let events = handle.state::<AccountEvents>();
            let allowed = events.runtime.lock().unwrap().notification(&account);
            publish(&handle);
            if allowed {
                let (detail, channel) = {
                    let runtime = events.runtime.lock().unwrap();
                    (
                        runtime.preferences.detail,
                        runtime
                            .channel_names
                            .get(&account)
                            .cloned()
                            .unwrap_or_default(),
                    )
                };
                // Only opt-in previews reach the desktop. Notification content never enters
                // account state, diagnostics, frontend events or persistent storage.
                let (sender, body) = if detail == NotificationDetail::Preview {
                    (Some(title), Some(body))
                } else {
                    (None, None)
                };
                let text =
                    NotificationText::message(detail, &channel, sender.as_deref(), body.as_deref());
                let handle = handle.clone();
                let account = account.clone();
                let dispatcher = handle.clone();
                let _ = dispatcher.run_on_main_thread(move || {
                    gtk::glib::MainContext::default().spawn_local(async move {
                        let result = send_desktop_notification(&handle, &account, text).await;
                        {
                            let events = handle.state::<AccountEvents>();
                            let mut runtime = events.runtime.lock().unwrap();
                            if let Some(health) = runtime.accounts.get_mut(&account) {
                                if result != Ok(false)
                                    && health.notification_delivery == NotificationDelivery::Pending
                                {
                                    health.notification_delivery = if result.is_ok() {
                                        NotificationDelivery::Submitted
                                    } else {
                                        NotificationDelivery::Failed
                                    };
                                }
                            }
                        }
                        publish(&handle);
                    });
                });
            }
        },
        move || {
            let _ = activity.try_send(());
        },
        move || {
            page(&failure_handle, &failure_id, PageState::ProcessStopped);
            let _ = failure_handle.emit_to(
                "main",
                "account-error",
                "The account could not start securely. Lock and unlock the vault to retry.",
            );
        },
    )
}

/// Called on GTK's main context; D-Bus work and click handling are asynchronous.
pub async fn send_desktop_notification(
    app: &tauri::AppHandle,
    id: &str,
    text: NotificationText,
) -> Result<bool, &'static str> {
    let handle = app.clone();
    app.state::<AccountEvents>()
        .desktop
        .show(id, text, move |account| {
            if handle.get_webview(&format!("account-{account}")).is_none() {
                return;
            }
            if let Some(window) = handle.get_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
            let _ = handle.emit_to("main", "open-account", account);
        })
        .await
}

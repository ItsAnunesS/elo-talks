//! Native adapters for page health, generic notifications and desktop counters.
use crate::account_runtime::{AccountRuntime, PageState};
use dbus::{
    arg::{PropMap, Variant},
    channel::Sender,
};
use std::sync::{
    atomic::{AtomicU64, AtomicUsize, Ordering},
    mpsc, Arc, Mutex,
};
use tauri::{Emitter, Manager};
use webkit2gtk::{NotificationExt, WebViewExt};

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
    pub runtime: Mutex<AccountRuntime>,
    badge_count: Arc<AtomicU64>,
    badge_wake: mpsc::SyncSender<()>,
    notification_workers: Arc<AtomicUsize>,
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
            badge_count,
            badge_wake,
            notification_workers: Arc::new(AtomicUsize::new(0)),
        }
    }
}

pub fn publish(app: &tauri::AppHandle) {
    let events = app.state::<AccountEvents>();
    let runtime = events.runtime.lock().unwrap();
    let total = runtime.total();
    let _ = app.emit_to("main", "account-state", &*runtime);
    drop(runtime);
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
        let _ = tray.set_tooltip(Some(format!("Elo Talks · {total} pending notifications")));
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

pub fn attach(view: &webkit2gtk::WebView, app: &tauri::AppHandle, id: &str) {
    let handle = app.clone();
    let account = id.to_owned();
    view.connect_load_changed(move |_, event| match event {
        webkit2gtk::LoadEvent::Started => page(&handle, &account, PageState::Loading),
        webkit2gtk::LoadEvent::Finished => page(&handle, &account, PageState::Ready),
        _ => {}
    });
    let handle = app.clone();
    let account = id.to_owned();
    view.connect_load_failed(move |_, _, _, _| {
        // Never expose remote URLs, WebKit error descriptions, or page text.
        let is_tls = handle
            .state::<AccountEvents>()
            .runtime
            .lock()
            .unwrap()
            .accounts
            .get(&account)
            .is_some_and(|health| health.page == PageState::TlsFailed);
        if !is_tls {
            page(&handle, &account, PageState::LoadFailed);
        }
        true
    });
    let handle = app.clone();
    let account = id.to_owned();
    view.connect_load_failed_with_tls_errors(move |_, _, _, _| {
        page(&handle, &account, PageState::TlsFailed);
        // Handling the signal does not grant a certificate exception.
        true
    });
    let handle = app.clone();
    let account = id.to_owned();
    view.connect_web_process_terminated(move |_, _| {
        page(&handle, &account, PageState::ProcessStopped)
    });
    let handle = app.clone();
    let account = id.to_owned();
    view.connect_show_notification(move |_, notification| {
        let events = handle.state::<AccountEvents>();
        let allowed = events.runtime.lock().unwrap().notification(&account);
        publish(&handle);
        if allowed {
            show_notification(&handle, &account, events.notification_workers.clone());
        }
        notification.close();
        true
    });
}

struct NotificationWorker(Arc<AtomicUsize>);
impl Drop for NotificationWorker {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

fn show_notification(app: &tauri::AppHandle, id: &str, workers: Arc<AtomicUsize>) {
    // Bound waiting action handlers; bursts still update badges without spawning unbounded threads.
    if workers
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
            (count < 4).then_some(count + 1)
        })
        .is_err()
    {
        return;
    }
    let handle = app.clone();
    let account = id.to_owned();
    std::thread::spawn(move || {
        let _worker = NotificationWorker(workers);
        if let Ok(notification) = notify_rust::Notification::new()
            .appname("Elo Talks")
            .summary("Elo Talks")
            .body("You have a new message.")
            .hint(notify_rust::Hint::DesktopEntry(
                "dev.anuness.elotalks".into(),
            ))
            .timeout(8000)
            .action("default", "Open account")
            .show()
        {
            notification.wait_for_action(move |action| {
                if action == "default" {
                    let app = handle.clone();
                    let _ = handle.run_on_main_thread(move || {
                        if let Some(window) = app.get_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                        let _ = app.emit_to("main", "open-account", &account);
                    });
                }
            });
        }
    });
}

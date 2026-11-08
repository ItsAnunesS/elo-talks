//! Bounded isolated-world startup queries; no conversation content is returned.
use crate::{
    account_events::{self, AccountEvents},
    account_runtime::Readiness,
};
use std::time::{Duration, Instant};
use tauri::Manager;
pub fn attach(view: &tauri::Webview, app: &tauri::AppHandle, id: &str) {
    let generation = {
        let events = app.state::<AccountEvents>();
        let mut runtime = events.runtime.lock().unwrap();
        let Some(health) = runtime.accounts.get_mut(id) else {
            return;
        };
        if health.readiness_probe_active || health.readiness != Readiness::Preparing {
            return;
        }
        health.readiness_probe_active = true;
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
    let app = app.clone();
    let id = id.to_owned();
    std::thread::spawn(move || {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(40) {
            std::thread::sleep(Duration::from_millis(500));
            if app
                .state::<AccountEvents>()
                .runtime
                .lock()
                .unwrap()
                .accounts
                .get(&id)
                .is_none_or(|health| {
                    health.generation != generation || health.readiness != Readiness::Preparing
                })
            {
                break;
            }
            let readiness = match script
                .evaluate(&view, include_str!("readiness.js"))
                .and_then(|value| value.as_u64())
            {
                Some(1) => Readiness::Chats,
                Some(2) => Readiness::QrCode,
                _ => continue,
            };
            let handle = app.clone();
            let account = id.clone();
            let _ = app.run_on_main_thread(move || {
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
                    health.readiness = readiness;
                    health.readiness_probe_active = false;
                }
                if readiness == Readiness::QrCode {
                    crate::account_avatar::clear(&handle, &account);
                }
                account_events::publish(&handle);
            });
            break;
        }
        if let Some(health) = app
            .state::<AccountEvents>()
            .runtime
            .lock()
            .unwrap()
            .accounts
            .get_mut(&id)
            .filter(|health| health.generation == generation)
        {
            health.readiness_probe_active = false;
        }
    });
}

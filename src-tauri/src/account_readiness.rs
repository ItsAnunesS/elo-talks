//! Bounded, read-only startup probe in an isolated JavaScript world.
use crate::{
    account_events::{self, AccountEvents},
    account_runtime::Readiness,
};
use gtk::{glib, prelude::*};
use javascriptcore::ValueExt;
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};
use tauri::Manager;
use webkit2gtk::WebViewExt;

pub fn attach(view: &webkit2gtk::WebView, app: &tauri::AppHandle, id: &str) {
    let generation = Rc::new(Cell::new(0u64));
    let handle = app.clone();
    let account = id.to_owned();
    view.connect_load_changed(move |view, event| {
        if event != webkit2gtk::LoadEvent::Started {
            return;
        }
        generation.set(generation.get().wrapping_add(1));
        let current = generation.get();
        let generation = generation.clone();
        let weak = view.downgrade();
        let handle = handle.clone();
        let account = account.clone();
        let started = Instant::now();
        let pending = Rc::new(Cell::new(false));
        let cancellable = gtk::gio::Cancellable::new();
        glib::timeout_add_local(Duration::from_millis(500), move || {
            let Some(view) = weak.upgrade() else {
                cancellable.cancel();
                return glib::ControlFlow::Break;
            };
            if generation.get() != current || started.elapsed() > Duration::from_secs(40) {
                cancellable.cancel();
                return glib::ControlFlow::Break;
            }
            let ready = handle
                .state::<AccountEvents>()
                .runtime
                .lock()
                .unwrap()
                .accounts
                .get(&account)
                .is_some_and(|health| health.readiness != Readiness::Preparing);
            if ready {
                return glib::ControlFlow::Break;
            }
            if pending.get()
                || !view
                    .uri()
                    .and_then(|u| tauri::Url::parse(&u).ok())
                    .is_some_and(|u| crate::whatsapp::is_origin(&u))
            {
                return glib::ControlFlow::Continue;
            }
            pending.set(true);
            let pending = pending.clone();
            let handle = handle.clone();
            let account = account.clone();
            let generation = generation.clone();
            view.evaluate_javascript(
                include_str!("readiness.js"),
                None,
                Some("elo-startup-readiness"),
                Some(&cancellable),
                move |result| {
                    pending.set(false);
                    if generation.get() != current || started.elapsed() > Duration::from_secs(40) {
                        return;
                    }
                    let state = match result
                        .ok()
                        .filter(|value| value.is_number())
                        .map(|value| value.to_int32())
                    {
                        Some(1) => Readiness::Chats,
                        Some(2) => Readiness::QrCode,
                        _ => return,
                    };
                    if let Some(health) = handle
                        .state::<AccountEvents>()
                        .runtime
                        .lock()
                        .unwrap()
                        .accounts
                        .get_mut(&account)
                    {
                        health.readiness = state;
                    }
                    if state == Readiness::QrCode {
                        crate::account_avatar::clear(&handle, &account);
                    }
                    account_events::publish(&handle);
                },
            );
            glib::ControlFlow::Continue
        });
    });
}

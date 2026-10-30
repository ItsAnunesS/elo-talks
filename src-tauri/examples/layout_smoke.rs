//! Synthetic native regression, run only in an isolated Xvfb desktop.
#[allow(dead_code)]
#[path = "../src/account_events.rs"]
mod account_events;
#[path = "../src/account_permissions.rs"]
mod account_permissions;
#[path = "../src/account_readiness.rs"]
mod account_readiness;
#[allow(dead_code)]
#[path = "../src/account_runtime.rs"]
mod account_runtime;
#[path = "../src/desktop_notifications.rs"]
mod desktop_notifications;
#[allow(dead_code)]
#[path = "../src/notification_content.rs"]
mod notification_content;
#[path = "../src/ui_layout.rs"]
mod ui_layout;
#[path = "../src/whatsapp.rs"]
mod whatsapp;
use gtk::prelude::*;
use std::{cell::Cell, rc::Rc, time::Duration};
use tauri::{webview::WebviewBuilder, LogicalPosition, LogicalSize, Manager, WebviewUrl};
use webkit2gtk::{SettingsExt, WebContextExt, WebViewExt};

fn main() {
    tauri::Builder::default()
        .manage(account_events::AccountEvents::default())
        .setup(|app| {
            let window = tauri::window::WindowBuilder::new(app, "layout-test")
                .title("Elo Talks — synthetic layout regression")
                .inner_size(1200., 800.)
                .build()?;
            for (index, label) in ["layout-shell", "account-one", "account-two"]
                .into_iter()
                .enumerate()
            {
                let view = window.add_child(
                    WebviewBuilder::new(label, WebviewUrl::External("about:blank".parse()?)),
                    LogicalPosition::new(0., 0.),
                    LogicalSize::new(1200., 800.),
                )?;
                let handle = app.handle().clone();
                view.with_webview(move |platform| {
                    let webview = platform.inner();
                    assert!(webview.context().is_some_and(|context| context.is_sandbox_enabled()), "WebKit sandbox must remain enabled in the layout test");
                    ui_layout::attach(&webview, index != 0).unwrap();
                    if index != 0 {
                        handle.state::<account_events::AccountEvents>().runtime.lock().unwrap().accounts.insert(label.into(), account_runtime::AccountHealth::default());
                        account_events::attach(&webview, &handle, label);
                        account_permissions::attach(&webview);
                        account_readiness::attach(&webview, &handle, label);
                    }
                    webview.load_html(
                        if index == 0 {
                            "<body style='margin:0;background:#171522;color:white;font:24px sans-serif'><header style='height:72px'>Elo Talks — test account</header><aside style='width:304px'>Accounts<br>Settings<br>Lock vault</aside>"
                        } else if index == 1 {
                            "<body style='background:#243e38'>Synthetic notification source<script>setTimeout(() => { const side = document.createElement('div'); side.id = 'pane-side'; side.style.height = '20px'; document.body.append(side); }, 3000);Promise.all(['notifications','geolocation'].map(name => navigator.permissions.query({name}).then(result => result.state === 'granted' ? 'granted' : 'not-granted'))).then(states => document.title = states.join('|')); if(Notification.permission === 'granted') new Notification('Synthetic private sender', {body:'Synthetic private message'});</script>"
                        } else {
                            "<body style='background:#243e38;color:white;font:32px sans-serif'>Synthetic conversation area — no personal data<div id=\"pane-side\">Untrusted structure</div><script>navigator.permissions.query({name:'notifications'}).then(result => document.title = Notification.permission + '|' + result.state);</script></body>"
                        },
                        Some(match index { 1 => "https://web.whatsapp.com/", 2 => "https://untrusted.example/", _ => "about:blank" }),
                    );
                    if index != 2 {
                        return;
                    }
                    let overlay = webview.parent().unwrap().downcast::<gtk::Overlay>().unwrap();
                    let children = overlay.children();
                    assert_eq!(children.len(), 3);
                    let shell = children[0].clone();
                    let first = children[1].clone();
                    let second = children[2].clone();
                    let native = overlay.toplevel().unwrap().downcast::<gtk::Window>().unwrap();
                    let first_view = handle.get_webview("account-one").unwrap();
                    let second_view = handle.get_webview("account-two").unwrap();
                    let stage = Rc::new(Cell::new(0));
                    let hidden_waits = Rc::new(Cell::new(0));
                    gtk::glib::timeout_add_local(Duration::from_millis(600), move || {
                        let check = |account: &gtk::Widget| {
                            let allocation = overlay.allocation();
                            assert_eq!(shell.allocated_width(), allocation.width());
                            assert_eq!(shell.allocated_height(), allocation.height());
                            assert_eq!(account.translate_coordinates(&shell, 0, 0), Some((304, 72)));
                            assert_eq!(account.allocated_width(), allocation.width() - 304);
                            assert_eq!(account.allocated_height(), allocation.height() - 72);
                            let view = account.clone().downcast::<webkit2gtk::WebView>().unwrap();
                            assert!(!WebViewExt::settings(&view).unwrap().enables_fullscreen());
                            assert!(view.emit_by_name::<bool>("enter-fullscreen", &[]));
                        };
                        match stage.get() {
                            0 => { assert!(!first.is_visible() && !second.is_visible()); ui_layout::set_account_visible(&first_view, true).unwrap(); }
                            1 => {
                                assert_eq!(handle.state::<account_events::AccountEvents>().runtime.lock().unwrap().accounts["account-one"].readiness, account_runtime::Readiness::Preparing, "Document load is not WhatsApp readiness");
                                assert_eq!(handle.state::<account_events::AccountEvents>().runtime.lock().unwrap().accounts["account-one"].page, account_runtime::PageState::Ready);
                                assert_eq!(handle.state::<account_events::AccountEvents>().runtime.lock().unwrap().accounts["account-one"].native_notifications, 1, "WhatsApp notification permission must be initialized before the page checks it");
                                let native_view = first.clone().downcast::<webkit2gtk::WebView>().unwrap();
                                assert_eq!(native_view.title().as_deref(), Some("granted|not-granted"), "Permissions API must grant WhatsApp notifications without granting geolocation");
                                native_view.emit_by_name::<()>("web-process-terminated", &[&webkit2gtk::WebProcessTerminationReason::Crashed]);
                                native_view.emit_by_name::<()>("load-changed", &[&webkit2gtk::LoadEvent::Finished]);
                                assert_eq!(handle.state::<account_events::AccountEvents>().runtime.lock().unwrap().accounts["account-one"].page, account_runtime::PageState::ProcessStopped);
                                check(&first); assert!(first.is_visible()); ui_layout::set_account_visible(&first_view, false).unwrap(); ui_layout::set_account_visible(&second_view, true).unwrap(); }
                            2 => {
                                assert_eq!(handle.state::<account_events::AccountEvents>().runtime.lock().unwrap().accounts["account-two"].readiness, account_runtime::Readiness::Preparing, "Untrusted origins cannot report readiness");
                                // WebKit masks a denied query as prompt before the origin
                                // has requested notifications; neither API may grant access.
                                assert_eq!(second.clone().downcast::<webkit2gtk::WebView>().unwrap().title().as_deref(), Some("default|prompt"), "Other origins must not inherit WhatsApp notification permission");
                                check(&second); assert!(second.is_visible()); assert!(!first.is_visible()); native.resize(900, 640);
                            }
                            3 => { check(&second); assert_eq!(overlay.allocated_width(), 900); native.resize(1400, 900); }
                            4 => {
                                check(&second);
                                assert_eq!(overlay.allocated_width(), 1400);
                                ui_layout::set_account_visible(&second_view, false).unwrap();
                                assert!(shell.is_visible());
                                native.hide();
                                let view = first.clone().downcast::<webkit2gtk::WebView>().unwrap();
                                view.evaluate_javascript("setTimeout(() => { document.title = '(7) WhatsApp'; new Notification('Synthetic background sender', {body:'Synthetic private body'}); }, 1000)", None, None, None::<&gtk::gio::Cancellable>, |_| {});
                            }
                            5 => {
                                let received = handle.state::<account_events::AccountEvents>().runtime.lock().unwrap().accounts["account-one"].native_notifications;
                                if handle.state::<account_events::AccountEvents>().runtime.lock().unwrap().accounts["account-one"].readiness != account_runtime::Readiness::Chats || received < 2 || handle.state::<account_events::AccountEvents>().runtime.lock().unwrap().accounts["account-one"].unread_count != Some(7) {
                                    hidden_waits.set(hidden_waits.get() + 1);
                                    assert!(hidden_waits.get() < 20, "Hidden account in hidden window must keep receiving native notification events");
                                    return gtk::glib::ControlFlow::Continue;
                                }
                                assert!(!first.is_visible() && !second.is_visible());
                                assert!(!native.is_visible());
                                {
                                    let events = handle.state::<account_events::AccountEvents>();
                                    let mut runtime = events.runtime.lock().unwrap();
                                    runtime.focused = true;
                                    runtime.selected = Some("account-one".into());
                                    runtime.acknowledge_selected();
                                    assert_eq!(runtime.total(), 7, "Selecting a channel must not mark its WhatsApp items as read");
                                }
                                first.clone().downcast::<webkit2gtk::WebView>().unwrap().evaluate_javascript("document.title = 'WhatsApp'", None, None, None::<&gtk::gio::Cancellable>, |_| {});
                                native.show();
                            }
                            6 => {
                                assert_eq!(handle.state::<account_events::AccountEvents>().runtime.lock().unwrap().accounts["account-one"].unread_count, Some(0));
                                assert!(shell.is_visible());
                                assert!(!first.is_visible() && !second.is_visible());
                                println!("PASS: shell bounds, account switching, fullscreen denial, consistent notification permissions with origin/feature restrictions and notification events from a hidden account/window");
                                std::process::exit(0);
                            }
                            _ => unreachable!(),
                        }
                        stage.set(stage.get() + 1);
                        gtk::glib::ControlFlow::Continue
                    });
                })?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .unwrap();
}

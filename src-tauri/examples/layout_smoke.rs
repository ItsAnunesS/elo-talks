//! Synthetic native regression, run only in an isolated Xvfb desktop.
#[allow(dead_code)]
#[path = "../src/account_events.rs"]
mod account_events;
#[allow(dead_code)]
#[path = "../src/account_runtime.rs"]
mod account_runtime;
#[path = "../src/ui_layout.rs"]
mod ui_layout;
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
                    }
                    webview.load_html(
                        if index == 0 {
                            "<body style='margin:0;background:#171522;color:white;font:24px sans-serif'><header style='height:72px'>Elo Talks — test account</header><aside style='width:304px'>Accounts<br>Settings<br>Lock vault</aside>"
                        } else if index == 1 {
                            "<body style='background:#243e38'><div id='app'><nav role='navigation'><button aria-label='Profile'><img src='data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z/D/PwAG/gL+DHWJ3gAAAABJRU5ErkJggg=='></button></nav></div><script>HTMLCanvasElement.prototype.getContext = () => { throw Error('Page-world canvas must not be used'); };</script>"
                        } else {
                            "<body style='background:#243e38;color:white;font:32px sans-serif'>Synthetic conversation area — no personal data</body>"
                        },
                        Some(if index == 1 { "https://web.whatsapp.com/" } else { "about:blank" }),
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
                    let avatar_checked = Rc::new(Cell::new(false));
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
                                assert_eq!(handle.state::<account_events::AccountEvents>().runtime.lock().unwrap().accounts["account-one"].page, account_runtime::PageState::Ready);
                                let native_view = first.clone().downcast::<webkit2gtk::WebView>().unwrap();
                                let checked = avatar_checked.clone();
                                native_view.evaluate_javascript(include_str!("../src/avatar-probe.js"), Some("dev.anuness.elotalks.avatar"), None, None::<&gtk::gio::Cancellable>, move |result| {
                                    let pixels = result.unwrap().to_string();
                                    assert_eq!(pixels.len(), 64 * 64 * 4 * 2);
                                    assert_eq!(&pixels[..8], "ff00ffff");
                                    checked.set(true);
                                });
                                native_view.emit_by_name::<()>("web-process-terminated", &[&webkit2gtk::WebProcessTerminationReason::Crashed]);
                                native_view.emit_by_name::<()>("load-changed", &[&webkit2gtk::LoadEvent::Finished]);
                                assert_eq!(handle.state::<account_events::AccountEvents>().runtime.lock().unwrap().accounts["account-one"].page, account_runtime::PageState::ProcessStopped);
                                check(&first); assert!(first.is_visible()); ui_layout::set_account_visible(&first_view, false).unwrap(); ui_layout::set_account_visible(&second_view, true).unwrap(); }
                            2 => { check(&second); assert!(second.is_visible()); assert!(!first.is_visible()); native.resize(900, 640); }
                            3 => { check(&second); assert_eq!(overlay.allocated_width(), 900); native.resize(1400, 900); }
                            4 => { check(&second); assert_eq!(overlay.allocated_width(), 1400); ui_layout::set_account_visible(&second_view, false).unwrap(); }
                            5 => {
                                assert!(!first.is_visible() && !second.is_visible());
                                assert!(shell.is_visible());
                                assert!(avatar_checked.get(), "Isolated-world avatar extraction did not complete");
                                println!("PASS: shell visible; two accounts bounded at (304,72); switching, shrink/grow, overview and fullscreen denial");
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

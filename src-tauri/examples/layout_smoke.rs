//! Synthetic native regression, run only in an isolated Xvfb desktop.
#[path = "../src/ui_layout.rs"]
mod ui_layout;
use gtk::prelude::*;
use std::{cell::Cell, rc::Rc, time::Duration};
use tauri::{webview::WebviewBuilder, LogicalPosition, LogicalSize, WebviewUrl};
use webkit2gtk::{SettingsExt, WebViewExt};

fn main() {
    tauri::Builder::default()
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
                view.with_webview(move |platform| {
                    let webview = platform.inner();
                    ui_layout::attach(&webview, index != 0).unwrap();
                    webview.load_html(
                        if index == 0 {
                            "<body style='margin:0;background:#171522;color:white;font:24px sans-serif'><header style='height:72px'>Elo Talks — conta de teste</header><aside style='width:304px'>Contas<br>Configurações<br>Bloquear cofre</aside>"
                        } else {
                            "<body style='background:#243e38;color:white;font:32px sans-serif'>Área de conversa sintética — sem dados pessoais</body>"
                        },
                        Some("about:blank"),
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
                    let stage = Rc::new(Cell::new(0));
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
                            0 => { assert!(!first.is_visible() && !second.is_visible()); first.show(); }
                            1 => { check(&first); first.hide(); second.show(); }
                            2 => { check(&second); assert!(!first.is_visible()); native.resize(900, 640); }
                            3 => { check(&second); assert_eq!(overlay.allocated_width(), 900); native.resize(1400, 900); }
                            4 => { check(&second); assert_eq!(overlay.allocated_width(), 1400); second.hide(); }
                            5 => {
                                assert!(!first.is_visible() && !second.is_visible());
                                assert!(shell.is_visible());
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

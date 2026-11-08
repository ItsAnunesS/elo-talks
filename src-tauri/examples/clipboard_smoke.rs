//! Native Chromium paste with synthetic clipboard data on an isolated X11 display.
#[path = "support/cef_fixture.rs"]
mod fixture;
use gtk::prelude::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use tauri::{webview::WebviewBuilder, LogicalSize, Manager, WebviewUrl};
use tauri_runtime_cef::{Cef, DebugEnvironment, DevToolsPolicy, RemoteDebugging, SandboxPolicy};
const HTML: &str = r#"<!doctype html><body style="margin:0"><div id="editor" contenteditable style="width:500px;height:400px"></div><script>window.pastes=[];editor.addEventListener('paste',event=>{event.preventDefault();const record={text:event.clipboardData.getData('text/plain'),files:[...event.clipboardData.files].map(file=>({type:file.type,size:file.size}))};pastes.push(record);const file=event.clipboardData.files[0];if(file)createImageBitmap(file).then(image=>{record.decoded=[image.width,image.height];image.close();}).catch(()=>record.invalid=true);});editor.focus();</script>"#;
fn paste() {
    let program = std::env::var_os("XDOTOOL").unwrap_or_else(|| "xdotool".into());
    assert!(std::process::Command::new(program)
        .args([
            "search",
            "--sync",
            "--onlyvisible",
            "--pid",
            &std::process::id().to_string(),
            "--name",
            "Clipboard fixture",
            "windowfocus",
            "--sync",
            "mousemove",
            "--window",
            "%1",
            "30",
            "40",
            "click",
            "1",
            "key",
            "--clearmodifiers",
            "ctrl+v"
        ])
        .status()
        .unwrap()
        .success());
}
fn clipboard(app: &tauri::AppHandle, kind: u8) {
    let (sender, receiver) = mpsc::sync_channel(1);
    app.run_on_main_thread(move || {
        let clipboard = gtk::gdk::Display::default().unwrap().clipboard();
        match kind {
            0 => {
                let image =
                    gtk::gdk_pixbuf::Pixbuf::new(gtk::gdk_pixbuf::Colorspace::Rgb, true, 8, 32, 32)
                        .unwrap();
                image.fill(0x336699ff);
                clipboard.set_texture(&gtk::gdk::Texture::for_pixbuf(&image));
            }
            1 => clipboard.set_text("Synthetic clipboard text"),
            _ => {
                let provider = gtk::gdk::ContentProvider::for_bytes(
                    "image/png",
                    &gtk::glib::Bytes::from_static(b"invalid synthetic image"),
                );
                clipboard.set_content(Some(&provider)).unwrap();
            }
        }
        sender.send(()).unwrap();
    })
    .unwrap();
    receiver
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap();
}
#[tauri_runtime_cef::cef_entry_point]
fn main() {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        hook(panic);
        std::process::exit(1);
    }));
    let root = tempfile::tempdir().unwrap();
    let passed = Arc::new(AtomicBool::new(false));
    let verified = passed.clone();
    tauri::Builder::default()
        .runtime(
            Cef::default()
                .root_cache_path(root.path())
                .sandbox(SandboxPolicy::Required)
                .debug_environment(DebugEnvironment::Deny)
                .remote_debugging(RemoteDebugging::Disabled)
                .devtools(DevToolsPolicy::Disallowed),
        )
        .setup(move |app| {
            let window = tauri::window::WindowBuilder::new(app, "clipboard")
                .title("Clipboard fixture")
                .inner_size(900., 640.)
                .build()?;
            let first = window.add_child(
                WebviewBuilder::new("account-one", WebviewUrl::External("about:blank".parse()?))
                    .data_directory(root.path().join("one"))
                    .size(LogicalSize::new(900., 640.)),
            )?;
            let second = window.add_child(
                WebviewBuilder::new("account-two", WebviewUrl::External("about:blank".parse()?))
                    .data_directory(root.path().join("two"))
                    .size(LogicalSize::new(900., 640.)),
            )?;
            second.hide()?;
            let app = app.handle().clone();
            std::thread::spawn(move || {
                for view in [&first, &second] {
                    fixture::serve(view, HTML);
                    view.navigate("https://web.whatsapp.com/".parse().unwrap())
                        .unwrap();
                    fixture::wait_for(|| fixture::evaluate(view, "typeof pastes") == "object");
                }
                first.set_focus().unwrap();
                clipboard(&app, 0);
                paste();
                fixture::wait_for(|| fixture::evaluate(&first, "pastes.length") == 1);
                assert_eq!(
                    fixture::evaluate(&first, "pastes[0].files[0].type"),
                    "image/png"
                );
                assert!(
                    fixture::evaluate(&first, "pastes[0].files[0].size")
                        .as_u64()
                        .unwrap()
                        > 0
                );
                clipboard(&app, 1);
                paste();
                fixture::wait_for(|| fixture::evaluate(&first, "pastes.length") == 2);
                assert_eq!(
                    fixture::evaluate(&first, "pastes[1].text"),
                    "Synthetic clipboard text"
                );
                clipboard(&app, 2);
                paste();
                fixture::wait_for(|| fixture::evaluate(&first,"pastes[2]?.invalid === true") == true);
                assert_eq!(fixture::evaluate(&first,"pastes[0].decoded"), serde_json::json!([32,32]));
                second.show().unwrap();
                second.set_focus().unwrap();
                first.hide().unwrap();
                let previous = fixture::evaluate(&first, "pastes.length");
                clipboard(&app, 1);
                paste();
                fixture::wait_for(|| fixture::evaluate(&second, "pastes.length") == 1);
                assert_eq!(
                    fixture::evaluate(&first, "pastes.length"),
                    previous,
                    "Hidden account must never receive the active account's paste"
                );
                assert_eq!(
                    fixture::evaluate(&second, "pastes[0].text"),
                    "Synthetic clipboard text"
                );
                fixture::evaluate(&second,"window.nativeDenied=false;window.__TAURI__.core.invoke('plugin:window|destroy',{label:'clipboard'}).catch(()=>window.nativeDenied=true);true");
                fixture::wait_for(|| fixture::evaluate(&second,"window.nativeDenied") == true);
                passed.store(true, Ordering::SeqCst);
                app.exit(0);
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .unwrap();
    assert!(verified.load(Ordering::SeqCst));
    println!(
        "PASS: CEF native image/text paste, invalid image handling and hidden-account isolation"
    );
}

//! Native Chromium paste with synthetic clipboard data on an isolated X11 display.
#[path = "../src/account_attachments.rs"]
mod account_attachments;
#[allow(dead_code)]
#[path = "../src/account_script.rs"]
mod account_script;
#[path = "support/cef_fixture.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "../src/whatsapp.rs"]
mod whatsapp;
use gtk::prelude::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use tauri::{webview::WebviewBuilder, LogicalSize, Manager, WebviewUrl};
use tauri_runtime_cef::{
    Cef, DebugEnvironment, DevToolsPolicy, RemoteDebugging, SandboxPolicy, WebviewBuilderCefExt,
    WebviewCefExt,
};
const HTML: &str = r#"<!doctype html><body style="margin:0"><div id="editor" contenteditable style="width:500px;height:400px"></div><script>window.pastes=[];editor.addEventListener('paste',event=>{event.preventDefault();const record={text:event.clipboardData.getData('text/plain'),files:[...event.clipboardData.files].map(file=>({name:file.name,type:file.type,size:file.size}))};pastes.push(record);const file=event.clipboardData.files[0];if(file)createImageBitmap(file).then(image=>{record.decoded=[image.width,image.height];image.close();}).catch(()=>record.invalid=true);});window.drops=[];window.nativeDrops=0;window.dragEvents={enter:0,over:0,leave:0};document.addEventListener('dragenter',()=>dragEvents.enter++);document.addEventListener('dragleave',()=>dragEvents.leave++);document.addEventListener('dragover',event=>{dragEvents.over++;if([...event.dataTransfer.types].includes('Files')){event.preventDefault();event.dataTransfer.dropEffect='copy';}});document.addEventListener('drop',event=>{event.preventDefault();if(event.isTrusted)nativeDrops++;drops.push([...event.dataTransfer.files].map(file=>({name:file.name,type:file.type,size:file.size})));});editor.focus();</script>"#;
fn paste(view: &tauri::Webview) {
    let (sender, receiver) = mpsc::sync_channel(1);
    view.with_cef_webview(move |native| {
        use tauri_runtime_cef::cef::{ImplBrowser, ImplBrowserHost};
        sender
            .send(native.browser().host().unwrap().window_handle())
            .unwrap();
    })
    .unwrap();
    let browser_window = receiver
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap();
    assert_ne!(browser_window, 0);
    // Target the browser child, not GTK's decoration/header coordinates. The
    // previous top-level (30, 40) click could focus chrome instead of the editor.
    let program = std::env::var_os("XDOTOOL").unwrap_or_else(|| "xdotool".into());
    assert!(std::process::Command::new("timeout")
        .arg("8s")
        .arg(&program)
        .args([
            "search",
            "--onlyvisible",
            "--pid",
            &std::process::id().to_string(),
            "--name",
            "Clipboard fixture",
            "windowfocus",
            "--sync",
            "mousemove",
            "--window",
            &browser_window.to_string(),
            "100",
            "100",
            "click",
            "1",
        ])
        .status()
        .unwrap()
        .success());
    fixture::wait_for(|| {
        fixture::evaluate(
            view,
            "document.hasFocus() && document.activeElement === editor",
        ) == true
    });
    assert!(std::process::Command::new("timeout")
        .arg("8s")
        .arg(program)
        .args(["key", "--clearmodifiers", "ctrl+v"])
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
fn drag_files() {
    assert!(std::process::Command::new("timeout")
        .args([
            "8s",
            "xdotool",
            "search",
            "--onlyvisible",
            "--pid",
            &std::process::id().to_string(),
            "--name",
            "Attachment source",
            "windowmove",
            "%1",
            "1100",
            "20",
            "mousemove",
            "--window",
            "%1",
            "30",
            "50",
            "mousedown",
            "1",
            "sleep",
            "0.1",
            "mousemove",
            "--window",
            "%1",
            "60",
            "50",
            "sleep",
            "0.3",
            "mousemove",
            "1200",
            "200",
            "sleep",
            "0.1",
            "mousemove",
            "200",
            "200",
            "sleep",
            "0.3",
            "mouseup",
            "1",
        ])
        .status()
        .unwrap()
        .success());
}

fn clipboard_files(
    app: &tauri::AppHandle,
    first: &std::path::Path,
    second: &std::path::Path,
    mime: &'static str,
    prefix: &str,
) {
    let content = format!(
        "{}{}\n{}\n",
        prefix,
        tauri::Url::from_file_path(first).unwrap(),
        tauri::Url::from_file_path(second).unwrap()
    );
    let (sender, receiver) = mpsc::sync_channel(1);
    app.run_on_main_thread(move || {
        let clipboard = gtk::gdk::Display::default().unwrap().clipboard();
        let provider = gtk::gdk::ContentProvider::for_bytes(
            mime,
            &gtk::glib::Bytes::from_owned(content.into_bytes()),
        );
        clipboard.set_content(Some(&provider)).unwrap();
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
            let active_first = Arc::new(AtomicBool::new(true));
            let first_slot = Arc::new(std::sync::Mutex::new(None::<Arc<account_attachments::Attachments>>));
            let second_slot = Arc::new(std::sync::Mutex::new(None::<Arc<account_attachments::Attachments>>));
            let first_paste = first_slot.clone();
            let second_paste = second_slot.clone();
            let first = window.add_child(
                WebviewBuilder::new("account-one", WebviewUrl::External("about:blank".parse()?))
                    .browser_runtime_style(tauri_runtime_cef::RuntimeStyle::Alloy)
                    .data_directory(root.path().join("one"))
                    .on_native_paste(move |browser| {
                        eprintln!("STAGE: first-account native paste hook");
                        first_paste.lock().unwrap().clone().is_none_or(|transfer| transfer.paste(browser))
                    })
                    .size(LogicalSize::new(900., 640.)),
            )?;
            let second = window.add_child(
                WebviewBuilder::new("account-two", WebviewUrl::External("about:blank".parse()?))
                    .browser_runtime_style(tauri_runtime_cef::RuntimeStyle::Alloy)
                    .data_directory(root.path().join("two"))
                    .on_native_paste(move |browser| {
                        eprintln!("STAGE: second-account native paste hook");
                        second_paste.lock().unwrap().clone().is_none_or(|transfer| transfer.paste(browser))
                    })
                    .size(LogicalSize::new(900., 640.)),
            )?;
            second.hide()?;
            let forbidden = root.path().join("vault");
            std::fs::create_dir(&forbidden).unwrap();
            let first_active = active_first.clone();
            let second_active = active_first.clone();
            let first_transfer = account_attachments::Attachments::attach(&first, forbidden.clone(), move || first_active.load(Ordering::SeqCst))?;
            let second_transfer = account_attachments::Attachments::attach(&second, forbidden, move || !second_active.load(Ordering::SeqCst))?;
            *first_slot.lock().unwrap() = Some(first_transfer.clone());
            *second_slot.lock().unwrap() = Some(second_transfer);
            let image = root.path().join("synthetic.png");
            let archive = root.path().join("synthetic.zip");
            std::fs::write(&image, include_bytes!("../icons/32x32.png"))?;
            std::fs::write(&archive, b"PK\x03\x04synthetic attachment")?;

            let drop_transfer = first_transfer.clone();
            let native_deliveries = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let drop_count = native_deliveries.clone();
            let drop_widget = window.gtk_window()?.child().unwrap();
            account_attachments::install_native_drop(&drop_widget, move |paths, position| {
                eprintln!("STAGE: GTK native file drop received");
                drop_count.fetch_add(1, Ordering::SeqCst);
                drop_transfer.drop_at(paths, position)
            });
            let drag_content = format!("{}\n{}\n", tauri::Url::from_file_path(&image).unwrap(), tauri::Url::from_file_path(&archive).unwrap());
            let source_button = gtk::Button::with_label("Drag synthetic attachments");
            let drag = gtk::DragSource::builder().actions(gtk::gdk::DragAction::COPY).build();
            let drag_started = Arc::new(AtomicBool::new(false));
            let drag_began = drag_started.clone();
            drag.connect_drag_begin(move |_, _| {
                drag_began.store(true, Ordering::SeqCst);
                eprintln!("STAGE: GTK native drag began");
            });
            drag.connect_drag_cancel(|_, _, reason| {
                eprintln!("STAGE: GTK native drag cancelled ({reason:?})");
                false
            });
            drag.connect_drag_end(|_, _, _| eprintln!("STAGE: GTK native drag ended"));
            drag.connect_prepare(move |_, _, _| Some(gtk::gdk::ContentProvider::for_bytes("text/uri-list", &gtk::glib::Bytes::from_owned(drag_content.as_bytes().to_vec()))));
            source_button.add_controller(drag);
            let source_window = gtk::Window::builder().title("Attachment source").decorated(false).default_width(200).default_height(120).child(&source_button).build();
            source_window.set_application(window.gtk_window()?.application().as_ref());
            source_window.present();
            let app = app.handle().clone();
            std::thread::spawn(move || {
                let _root = root;
                for view in [&first, &second] {
                    fixture::serve(view, HTML);
                    view.navigate("https://web.whatsapp.com/".parse().unwrap())
                        .unwrap();
                    fixture::wait_for(|| fixture::evaluate(view, "typeof pastes") == "object");
                }
                assert!(std::process::Command::new("timeout").args([
                    "8s", "xdotool", "search", "--onlyvisible", "--pid", &std::process::id().to_string(),
                    "--name", "Attachment source", "windowmove", "%1", "1100", "20",
                ]).status().unwrap().success());
                first.set_focus().unwrap();
                eprintln!("STAGE: image paste into active editor");
                clipboard(&app, 0);
                paste(&first);
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
                eprintln!("STAGE: text paste into active editor");
                clipboard(&app, 1);
                paste(&first);
                fixture::wait_for(|| fixture::evaluate(&first, "pastes.length") == 2);
                assert_eq!(
                    fixture::evaluate(&first, "pastes[1].text"),
                    "Synthetic clipboard text"
                );
                eprintln!("STAGE: malformed image paste");
                clipboard(&app, 2);
                paste(&first);
                fixture::wait_for(|| fixture::evaluate(&first,"pastes[2]?.invalid === true") == true);
                assert_eq!(fixture::evaluate(&first,"pastes[0].decoded"), serde_json::json!([32,32]));
                for (mime, prefix, expected) in [("text/uri-list", "", 4), ("x-special/gnome-copied-files", "copy\n", 5)] {
                    eprintln!("STAGE: native file-list paste {mime}");
                    clipboard_files(&app, &image, &archive, mime, prefix);
                    let before = fixture::evaluate(&first,"pastes.length");
                    fixture::evaluate(&first,"editor.dispatchEvent(new KeyboardEvent('keydown',{key:'v',ctrlKey:true,bubbles:true}));true");
                    assert_eq!(fixture::evaluate(&first,"pastes.length"),before,"Page-generated keys cannot request clipboard files");
                    paste(&first);
                    fixture::wait_for(|| fixture::evaluate(&first,"pastes.length") == expected);
                    assert_eq!(fixture::evaluate(&first,&format!("pastes[{}].files.map(file=>file.name)",expected-1)),serde_json::json!(["synthetic.png","synthetic.zip"]));
                }
                eprintln!("STAGE: adapter file drop");
                first_transfer.drop_at(vec![image.clone(),archive], tauri::PhysicalPosition::new(100., 100.));
                fixture::wait_for(|| fixture::evaluate(&first,"drops.length") == 1);
                assert_eq!(fixture::evaluate(&first,"drops[0].map(file=>file.name)"),serde_json::json!(["synthetic.png","synthetic.zip"]));
                eprintln!("STAGE: native file drag and drop");
                drag_files();
                assert!(drag_started.load(Ordering::SeqCst), "GTK source did not begin the native drag");
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
                loop {
                    let state = fixture::evaluate(&first,"({drops:drops.length,native:nativeDrops,events:dragEvents,focused:document.hasFocus(),hidden:document.hidden})");
                    if state["drops"].as_u64().unwrap() > 1 {
                        assert_eq!(state["drops"], 2, "Native drop was delivered more than once: {state}");
                        break;
                    }
                    assert!(std::time::Instant::now() < deadline, "Native file drop did not arrive: {state}");
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }
                assert_eq!(native_deliveries.load(Ordering::SeqCst), 1, "OS drag must deliver exactly one GTK drop");
                assert_eq!(fixture::evaluate(&first,"nativeDrops"), 0, "GTK delivery must not duplicate Chromium delivery");
                assert_eq!(fixture::evaluate(&first,"drops[1].map(file=>file.name)"),serde_json::json!(["synthetic.png","synthetic.zip"]));
                app.run_on_main_thread(|| {
                    for widget in gtk::Window::list_toplevels() {
                        if let Ok(window) = widget.downcast::<gtk::Window>() {
                            if window.title().as_deref() == Some("Attachment source") { window.close(); }
                        }
                    }
                }).unwrap();
                eprintln!("STAGE: hidden-account paste isolation");
                active_first.store(false,Ordering::SeqCst);
                second.show().unwrap();
                second.set_focus().unwrap();
                first.hide().unwrap();
                let previous = fixture::evaluate(&first, "pastes.length");
                clipboard_files(&app, &image, &image, "text/uri-list", "");
                paste(&second);
                fixture::wait_for(|| fixture::evaluate(&second, "pastes.length") == 1);
                assert_eq!(
                    fixture::evaluate(&first, "pastes.length"),
                    previous,
                    "Hidden account must never receive the active account's paste"
                );
                assert_eq!(
                    fixture::evaluate(&second, "pastes[0].files[0].name"),
                    "synthetic.png"
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
        "PASS: CEF image/text/file paste, attachment delivery, invalid image handling and hidden-account isolation"
    );
}

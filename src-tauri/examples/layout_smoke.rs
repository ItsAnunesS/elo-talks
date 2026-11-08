//! Synthetic native regression, run only in an isolated Xvfb desktop.
#[allow(dead_code)]
#[path = "../src/account_avatar.rs"]
mod account_avatar;
#[allow(dead_code)]
#[path = "../src/account_events.rs"]
mod account_events;
#[path = "../src/account_readiness.rs"]
mod account_readiness;
#[allow(dead_code)]
#[path = "../src/account_runtime.rs"]
mod account_runtime;
#[path = "../src/account_script.rs"]
mod account_script;
#[path = "../src/cef_notifications.rs"]
mod cef_notifications;
#[path = "../src/desktop_notifications.rs"]
mod desktop_notifications;
#[path = "support/cef_fixture.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "../src/notification_content.rs"]
mod notification_content;
#[allow(dead_code)]
#[path = "../src/storage.rs"]
mod storage;
#[path = "../src/ui_layout.rs"]
mod ui_layout;
#[allow(dead_code)]
#[path = "../src/whatsapp.rs"]
mod whatsapp;
#[path = "../src/whatsapp_style.rs"]
mod whatsapp_style;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use tauri::{webview::WebviewBuilder, LogicalPosition, LogicalSize, Manager, WebviewUrl};
use tauri_runtime_cef::{
    Cef, DebugEnvironment, DevToolsPolicy, RemoteDebugging, SandboxPolicy, WebviewCefExt,
};
const HTML: &str = r#"<!doctype html><body class="dark"><div id="pane-side" style="width:100px;height:100px">Synthetic structure</div><script>document.querySelector = () => null; window.fixtureAlive=1;setInterval(()=>window.fixtureAlive++,100);</script>"#;
#[tauri_runtime_cef::cef_entry_point]
fn main() {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        hook(panic);
        std::process::exit(1);
    }));
    let root = tempfile::tempdir().unwrap();
    let completed = Arc::new(AtomicBool::new(false));
    let verified = completed.clone();
    tauri::Builder::default()
        .runtime(Cef::default().root_cache_path(root.path()).sandbox(SandboxPolicy::Required).debug_environment(DebugEnvironment::Deny).remote_debugging(RemoteDebugging::Disabled).devtools(DevToolsPolicy::Disallowed).certificate_errors(tauri_runtime_cef::CertificateErrorPolicy::Cancel)
            .default_content_setting(tauri_runtime_cef::cef::ContentSettingTypes::NOTIFICATIONS, tauri_runtime_cef::cef::ContentSettingValues::BLOCK))
        .manage(account_events::AccountEvents::default())
        .setup(move |app| {
            let window = tauri::window::WindowBuilder::new(app,"layout-test").inner_size(1200.,800.).build()?;
            let shell = window.add_child(WebviewBuilder::new("main",WebviewUrl::App("index.html".into())).size(LogicalSize::new(1200.,800.)))?;
            ui_layout::attach(&window,&shell)?;
            let mut accounts = Vec::new();
            for id in ["one","two"] {
                app.state::<account_events::AccountEvents>().runtime.lock().unwrap().accounts.insert(id.into(),account_runtime::AccountHealth::default());
                let view = window.add_child(WebviewBuilder::new(format!("account-{id}"),WebviewUrl::External("about:blank".parse()?))
                    .data_directory(root.path().join(id)).devtools(false)
                    .on_page_load(|view,event| {
                        if event.event() == tauri::webview::PageLoadEvent::Finished && whatsapp::is_origin(event.url()) {
                            let id = view.label().strip_prefix("account-").unwrap();
                            account_events::page(view.app_handle(),id,account_runtime::PageState::Ready);
                            account_readiness::attach(&view,view.app_handle(),id);
                            account_avatar::attach(&view,view.app_handle(),id);
                            whatsapp_style::attach(&view);
                        }
                    })
                    .position(LogicalPosition::new(304.,108.)).size(LogicalSize::new(890.,686.)))?;
                view.hide()?;
                accounts.push(view);
            }
            let app = app.handle().clone();
            std::thread::spawn(move || {
                let [first,second] = accounts.as_slice() else { unreachable!() };
                let (activity,_receiver) = mpsc::sync_channel(1);
                eprintln!("STAGE: attach account adapters");
                for (view,id) in [(first,"one"),(second,"two")] {
                    let html = format!("{HTML}<script>const bytes=Uint8Array.from(atob('{}'),c=>c.charCodeAt(0));const own=document.createElement('button');own.title='You';own.style.cssText='position:fixed;left:16px;bottom:16px;width:48px;height:48px';const image=document.createElement('img');image.style.cssText='width:32px;height:32px';image.src=URL.createObjectURL(new Blob([bytes],{{type:'image/png'}}));own.append(image);document.body.append(own);</script>",gtk::glib::base64_encode(include_bytes!("../icons/32x32.png")));
                    let html = if id == "two" {
                        html.replace("image.src=URL.createObjectURL(new Blob([bytes],{type:'image/png'}));", "image.onload=()=>{const canvas=document.createElement('canvas');canvas.getContext('2d').drawImage(image,0,0);try{canvas.toDataURL();window.canvasBlocked=false;}catch{window.canvasBlocked=true;}};image.src='https://avatar.example/self.png';")
                    } else { html };
                    fixture::serve(view,&html);
                    account_avatar::observe(view,&app,id).unwrap();
                    account_events::attach(view,&app,id,activity.clone()).unwrap();
                    fixture::wait_for(|| fixture::evaluate(view,"location.origin") == "https://web.whatsapp.com");
                }
                eprintln!("STAGE: account readiness");
                fixture::wait_for(|| app.state::<account_events::AccountEvents>().runtime.lock().unwrap().accounts.values().all(|state| state.readiness == account_runtime::Readiness::Chats));
                eprintln!("STAGE: account avatars");
                fixture::wait_for(|| account_avatar::current(&app,"one").is_some() && account_avatar::current(&app,"two").is_some());
                assert_eq!(fixture::evaluate(second,"window.canvasBlocked"), true);
                assert!(!app.state::<account_events::AccountEvents>().runtime.lock().unwrap().diagnostics().to_string().contains("base64"));
                assert_eq!(fixture::evaluate(first,"localStorage.setItem('isolation','one');document.cookie='isolation=one';true"),true);
                assert_eq!(fixture::evaluate(second,"localStorage.getItem('isolation') === null && !document.cookie.includes('isolation=one')"),true);
                eprintln!("STAGE: switch native views");
                ui_layout::set_account_visible(first,true).unwrap();
                ui_layout::set_account_visible(first,false).unwrap();
                ui_layout::set_account_visible(second,true).unwrap();
                eprintln!("STAGE: resize native views");
                for (width,height) in [(900.,640.),(1400.,900.),(1200.,800.)] {
                    window.set_size(LogicalSize::new(width,height)).unwrap();
                    fixture::wait_for(|| {
                        let scale = window.scale_factor().unwrap();
                        let size = second.size().unwrap().to_logical::<f64>(scale);
                        (size.width-(width-310.)).abs() < 1. && (size.height-(height-114.)).abs() < 1.
                    });
                    let scale = window.scale_factor().unwrap();
                    assert_eq!(second.position().unwrap().to_logical::<f64>(scale),LogicalPosition::new(304.,108.));
                    assert_eq!(shell.size().unwrap(),window.inner_size().unwrap());
                }
                eprintln!("STAGE: hidden notification");
                window.hide().unwrap();
                fixture::evaluate(first,"new Notification('Synthetic sender',{body:'Synthetic body'});true");
                fixture::wait_for(|| app.state::<account_events::AccountEvents>().runtime.lock().unwrap().accounts["one"].native_notifications == 1);
                assert_eq!(fixture::evaluate(first,"getComputedStyle(document.body).getPropertyValue('--panel-header-background').trim()"),"#1a1a1e");
                eprintln!("STAGE: fullscreen rejection");
                window.show().unwrap(); second.set_focus().unwrap();
                let id = tauri_runtime_cef::allocate_devtools_message_id().unwrap();
                second.send_dev_tools_message(serde_json::json!({"id":id,"method":"Runtime.evaluate","params":{"expression":"document.documentElement.requestFullscreen().catch(()=>{})","userGesture":true}}).to_string().as_bytes()).unwrap();
                std::thread::sleep(std::time::Duration::from_millis(300));
                assert!(!window.is_fullscreen().unwrap());
                assert_eq!(fixture::evaluate(second,"!!document.fullscreenElement"),false);
                eprintln!("STAGE: cross-origin capability rejection");
                second.navigate("https://untrusted.example/".parse().unwrap()).unwrap();
                fixture::wait_for(|| fixture::evaluate(second,"location.origin") == "https://untrusted.example");
                assert_eq!(fixture::evaluate(second,"!!Notification.__elo"),false);
                fixture::evaluate(second,"window.nativeDenied=false;window.__TAURI__.core.invoke('plugin:window|destroy',{label:'layout-test'}).catch(()=>window.nativeDenied=true);true");
                fixture::wait_for(|| fixture::evaluate(second,"window.nativeDenied") == true);
                eprintln!("STAGE: clean shutdown");
                completed.store(true,Ordering::SeqCst);
                app.exit(0);
            });
            Ok(())
        }).run(tauri::generate_context!()).unwrap();
    assert!(verified.load(Ordering::SeqCst));
    println!("PASS: CEF account isolation, native geometry, switching, hidden notifications/readiness, theme, fullscreen and remote capability denial");
}

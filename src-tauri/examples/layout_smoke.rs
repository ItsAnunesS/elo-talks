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
#[path = "../src/notification_photo.rs"]
mod notification_photo;
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
    cef::{ImplBrowser, ImplBrowserHost},
    Cef, DebugEnvironment, DevToolsPolicy, RemoteDebugging, SandboxPolicy, WebviewCefExt,
};
const HTML: &str = r#"<!doctype html><body class="dark" style="margin:0;min-height:100vh;background:#e03050"><div id="pane-side" style="width:100px;height:100px">Synthetic structure</div><script>document.querySelector = () => null; window.fixtureAlive=1;setInterval(()=>window.fixtureAlive++,100);</script>"#;
fn native_window(view: &tauri::Webview) -> u64 {
    let (sender, receiver) = mpsc::sync_channel(1);
    view.with_cef_webview(move |native| {
        let window = native.browser().host().unwrap().window_handle();
        let _ = sender.send(window);
    })
    .unwrap();
    receiver
        .recv_timeout(std::time::Duration::from_secs(3))
        .unwrap()
}
fn presentation_probe(window: u64, action: &str) -> bool {
    std::process::Command::new("python3")
        .args([
            "-c",
            include_str!("support/presentation_probe.py"),
            &window.to_string(),
            action,
        ])
        .status()
        .unwrap()
        .success()
}

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
            for id in ["one","two","qr"] {
                app.state::<account_events::AccountEvents>().runtime.lock().unwrap().accounts.insert(id.into(),account_runtime::AccountHealth::default());
                let view = window.add_child(WebviewBuilder::new(format!("account-{id}"),WebviewUrl::External("about:blank".parse()?))
                    .data_directory(root.path().join(id)).devtools(false)
                    .on_page_load(|view,event| {
                        if !whatsapp::is_origin(event.url()) { return; }
                        let id = view.label().strip_prefix("account-").unwrap();
                        if event.event() == tauri::webview::PageLoadEvent::Finished {
                            account_events::page(view.app_handle(),id,account_runtime::PageState::Ready);
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
                let [first,second,qr] = accounts.as_slice() else { unreachable!() };
                let (activity,_receiver) = mpsc::sync_channel(1);
                eprintln!("STAGE: attach account adapters");
                for (view,id) in [(first,"one"),(second,"two"),(qr,"qr")] {
                    let html = format!("{HTML}<script>const bytes=Uint8Array.from(atob('{}'),c=>c.charCodeAt(0));const own=document.createElement('button');own.title='You';own.style.cssText='position:fixed;left:16px;bottom:16px;width:48px;height:48px';const image=document.createElement('img');image.style.cssText='width:32px;height:32px';image.src=URL.createObjectURL(new Blob([bytes],{{type:'image/png'}}));own.append(image);document.body.append(own);</script>",gtk::glib::base64_encode(include_bytes!("../icons/32x32.png")));
                    let html = if id == "two" {
                        html.replace("image.src=URL.createObjectURL(new Blob([bytes],{type:'image/png'}));", "image.onload=()=>{const canvas=document.createElement('canvas');canvas.getContext('2d').drawImage(image,0,0);try{canvas.toDataURL();window.canvasBlocked=false;}catch{window.canvasBlocked=true;}};image.src='https://avatar.example/self.png';")
                    } else { html };
                    let html = if id == "two" { html.replace("#e03050", "#30c060") } else { html };
                    let html = if id == "qr" {
                        r#"<!doctype html><body style="margin:0;min-height:100vh;background:#3070e0"><img src="https://web.whatsapp.com/synthetic-pending.png"><script>setTimeout(()=>{document.body.insertAdjacentHTML('beforeend','<div data-ref="synthetic" style="position:relative;width:160px;height:160px"><canvas width="160" height="160"></canvas><div style="position:absolute;left:60px;top:60px;width:40px;height:40px">Synthetic logo</div></div>');},750);document.querySelector=()=>null;</script>"#.to_owned()
                    } else { html };
                    fixture::serve(view,&html);
                    account_avatar::observe(view,&app,id).unwrap();
                }
                // Register the next account while the previous document can
                // still finish loading and query its readiness on the UI thread.
                for (view,id) in [(first,"one"),(second,"two"),(qr,"qr")] {
                    eprintln!("STAGE: attach account adapter {id}");
                    account_events::attach(view,&app,id,activity.clone()).unwrap();
                    fixture::wait_for(|| fixture::evaluate(view,"location.origin") == "https://web.whatsapp.com");
                }
                eprintln!("STAGE: account readiness");
                fixture::wait_for(|| {
                    let events = app.state::<account_events::AccountEvents>();
                    let runtime = events.runtime.lock().unwrap();
                    runtime.accounts.len() == 3 && runtime.accounts.iter().all(|(id,state)| state.page == if id == "qr" { account_runtime::PageState::Loading } else { account_runtime::PageState::Ready } && state.readiness == if id == "qr" { account_runtime::Readiness::QrCode } else { account_runtime::Readiness::Chats })
                });
                eprintln!("STAGE: same-document Business-style QR sign-in and logout");
                let qr_generation = app.state::<account_events::AccountEvents>().runtime.lock().unwrap().accounts["qr"].generation;
                fixture::evaluate(qr,"document.getElementsByTagName('canvas')[0].parentElement.remove();const chats=document.createElement('section');chats.id='pane-side';chats.style.cssText='width:300px;height:300px';document.body.append(chats);true");
                fixture::wait_for(|| app.state::<account_events::AccountEvents>().runtime.lock().unwrap().accounts["qr"].readiness == account_runtime::Readiness::Chats);
                assert_eq!(app.state::<account_events::AccountEvents>().runtime.lock().unwrap().accounts["qr"].generation,qr_generation,"QR sign-in must be recognized without a document reload");
                fixture::evaluate(qr,"document.getElementById('pane-side').remove();const login=document.createElement('div');login.dataset.ref='synthetic';login.append(document.createElement('canvas'));document.body.append(login);true");
                fixture::wait_for(|| app.state::<account_events::AccountEvents>().runtime.lock().unwrap().accounts["qr"].readiness == account_runtime::Readiness::QrCode);
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
                eprintln!("STAGE: visible pixels survive linked/unlinked account switching");
                let shell_window = native_window(&shell);
                let markers = [(first, "e03050"), (second, "30c060"), (qr, "3070e0")];
                let windows = markers.iter().map(|(view,_)| native_window(view)).collect::<Vec<_>>();
                for (index, (view,_)) in markers.iter().enumerate() {
                    fixture::evaluate(view,&format!("window.presentationToken={index};true"));
                }
                for selected in [0usize, 2, 1, 2, 0, 1, 2] {
                    // A local dialog/overview can raise the shell while browsers stay alive.
                    assert!(presentation_probe(shell_window, "raise"));
                    ui_layout::set_account_visible(markers[selected].0,true).unwrap();
                    for (index,(view,_)) in markers.iter().enumerate() {
                        if index != selected { ui_layout::set_account_visible(view,false).unwrap(); }
                    }
                    fixture::wait_for(|| presentation_probe(windows[selected],markers[selected].1));
                    for (index,(view,_)) in markers.iter().enumerate() {
                        assert_eq!(fixture::evaluate(view,"window.presentationToken"),serde_json::json!(index),"Switching views must not reload either linked or QR documents");
                    }
                }
                ui_layout::set_account_visible(second,true).unwrap();
                ui_layout::set_account_visible(qr,false).unwrap();
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

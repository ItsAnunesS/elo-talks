//! Real mouse reordering in CEF with offline, synthetic navigation items.
#[path = "support/cef_fixture.rs"]
mod fixture;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::{webview::WebviewBuilder, LogicalSize, WebviewUrl};
use tauri_runtime_cef::{Cef, DebugEnvironment, DevToolsPolicy, RemoteDebugging, SandboxPolicy};

fn mouse_input(arguments: &[&str]) {
    assert!(
        std::process::Command::new("timeout")
            .args([
                "8s",
                "xdotool",
                "search",
                "--onlyvisible",
                "--pid",
                &std::process::id().to_string(),
                "--name",
                "Channel drag fixture",
                "windowfocus",
            ])
            .args(arguments)
            .status()
            .unwrap()
            .success(),
        "Synthetic mouse input did not complete"
    );
}

fn pointer_sample(view: &tauri::Webview, coordinate: &str) -> [f64; 2] {
    fixture::evaluate(view, "window.pointerSample = null; true");
    mouse_input(&[
        "mousemove",
        "--window",
        "%1",
        coordinate,
        coordinate,
        "sleep",
        "0.1",
    ]);
    fixture::wait_for(|| fixture::evaluate(view, "Array.isArray(window.pointerSample)") == true);
    let sample = fixture::evaluate(view, "window.pointerSample");
    [sample[0].as_f64().unwrap(), sample[1].as_f64().unwrap()]
}

fn drag(view: &tauri::Webview, from: &str, to: &str) {
    // Calibrate X11 window coordinates against actual DOM pointer coordinates.
    // devicePixelRatio alone does not describe the GTK/CEF embedding transform.
    let first = pointer_sample(view, "24");
    let second = pointer_sample(view, "48");
    let scale = [24. / (second[0] - first[0]), 24. / (second[1] - first[1])];
    assert!(scale
        .iter()
        .all(|value| value.is_finite() && *value > 0. && *value <= 8.));
    eprintln!(
        "Synthetic pointer calibration: x={} y={}",
        scale[0], scale[1]
    );
    let coordinates = fixture::evaluate(
        view,
        &format!(
            r#"[{}, {}].map(selector => {{ const rect = document.querySelector(selector).getBoundingClientRect(); return [rect.x + 30, rect.y + 4]; }})"#,
            serde_json::json!(from),
            serde_json::json!(to)
        ),
    );
    let physical = |point: usize, axis: usize| {
        (24. + (coordinates[point][axis].as_f64().unwrap() - first[axis]) * scale[axis])
            .round()
            .to_string()
    };
    let x = physical(0, 0);
    let y = physical(0, 1);
    let tx = physical(1, 0);
    let ty = physical(1, 1);
    eprintln!("Synthetic drag stage: {from} -> {to}");
    mouse_input(&[
        "mousemove",
        "--window",
        "%1",
        &x,
        &y,
        "mousedown",
        "1",
        "sleep",
        "0.1",
        "mousemove",
        "--window",
        "%1",
        &x,
        &ty,
        "sleep",
        "0.2",
        "mousemove",
        "--window",
        "%1",
        &tx,
        &ty,
        "sleep",
        "0.2",
        "mouseup",
        "1",
    ]);
    eprintln!("Synthetic drag input completed: {from} -> {to}");
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
        .runtime(Cef::default().root_cache_path(root.path()).sandbox(SandboxPolicy::Required).debug_environment(DebugEnvironment::Deny).remote_debugging(RemoteDebugging::Disabled).devtools(DevToolsPolicy::Disallowed))
        .setup(move |app| {
            let window = tauri::window::WindowBuilder::new(app, "drag-fixture").title("Channel drag fixture").decorations(false).inner_size(900.,640.).build()?;
            let view = window.add_child(WebviewBuilder::new("drag-fixture",WebviewUrl::External("about:blank".parse()?)).data_directory(root.path().join("synthetic")).size(LogicalSize::new(900.,640.)))?;
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let script = include_str!("../../ui/channel-drag.js").replace("export function", "function");
                let html = format!(r#"<!doctype html><title>Channel drag fixture</title><style>body{{margin:0;user-select:none}} .workspace-icon,.account-row{{display:block;width:200px;height:64px;margin:8px;touch-action:none}} #channels{{position:absolute;left:240px;top:0}}</style><div id="workspaces"><button class="workspace-icon" id="w1" draggable="false" data-reorderable="true" data-workspace-id="w1" data-drop-workspace="w1">Workspace one</button><button class="workspace-icon" id="w2" draggable="false" data-reorderable="true" data-workspace-id="w2" data-drop-workspace="w2">Workspace two</button></div><div id="channels" data-drop-workspace="w1"><div class="account-row" id="a" draggable="false" data-reorderable="true" data-account-id="a"><button class="account-name">Synthetic one</button></div><div class="account-row" id="b" draggable="false" data-reorderable="true" data-account-id="b"><button class="account-name">Synthetic two</button></div></div><script>{script}
                window.moves=[];window.errors=[];window.counts={{}};document.addEventListener('pointermove',event=>window.pointerSample=[event.clientX,event.clientY]);for(const name of ['dragstart','dragover','drop','pointerdown','pointermove','pointerup','pointercancel'])document.addEventListener(name,()=>counts[name]=(counts[name]||0)+1);
                initializeChannelDrag(async(id,workspaceId,categoryId,beforeId)=>{{moves.push({{id,workspaceId,categoryId,beforeId}});channels.insertBefore(document.getElementById(id),beforeId?document.getElementById(beforeId):null);}},error=>errors.push(String(error)),async(id,beforeId)=>{{moves.push({{id,beforeId}});workspaces.insertBefore(document.getElementById(id),beforeId?document.getElementById(beforeId):null);}});window.ready=true;</script>"#);
                fixture::serve(&view,&html);
                view.navigate("https://web.whatsapp.com/".parse().unwrap()).unwrap();
                fixture::wait_for(|| fixture::evaluate(&view,"window.ready === true") == true);
                view.set_focus().unwrap();
                drag(&view,"#b","#a");
                eprintln!("Synthetic channel drag state: {}",fixture::evaluate(&view,"({counts, moves: moves.length, errors, dragging: !!document.querySelector('.is-dragging')})"));
                fixture::wait_for(|| fixture::evaluate(&view,"moves.length === 1") == true);
                assert_eq!(fixture::evaluate(&view,"channels.firstElementChild.id"),"b");
                eprintln!("PASS: native channel reorder");
                drag(&view,"#w2","#w1");
                eprintln!("Synthetic workspace drag state: {}",fixture::evaluate(&view,"({counts, moves: moves.length, errors, dragging: !!document.querySelector('.is-dragging')})"));
                fixture::wait_for(|| fixture::evaluate(&view,"moves.length === 2") == true);
                assert_eq!(fixture::evaluate(&view,"workspaces.firstElementChild.id"),"w2");
                eprintln!("PASS: native workspace reorder");
                assert_eq!(fixture::evaluate(&view,"errors.length"),0);
                passed.store(true,Ordering::SeqCst);
                handle.exit(0);
            });
            Ok(())
        }).run(tauri::generate_context!()).unwrap();
    assert!(verified.load(Ordering::SeqCst));
    println!("PASS: real CEF mouse input reorders synthetic channels and workspaces");
}

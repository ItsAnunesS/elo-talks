//! Offline native fixtures: every HTTP request is fulfilled locally, never sent.
use serde_json::{json, Value};
use std::{sync::mpsc, time::Duration};
use tauri_runtime_cef::{allocate_devtools_message_id, DevToolsProtocol, WebviewCefExt};

pub fn evaluate(view: &tauri::Webview, expression: &str) -> Value {
    let (sender, receiver) = mpsc::sync_channel(1);
    view.eval_with_callback(expression, move |result| {
        let _ = sender.try_send(result);
    })
    .unwrap();
    serde_json::from_str(
        &receiver
            .recv_timeout(Duration::from_secs(3))
            .expect("Synthetic renderer response"),
    )
    .unwrap()
}

pub fn serve(view: &tauri::Webview, html: &str) {
    let id = allocate_devtools_message_id().unwrap();
    let (ready, receiver) = mpsc::sync_channel(1);
    let (requests, incoming) = mpsc::sync_channel::<(String, bool, bool)>(128);
    view.on_dev_tools_protocol(move |event| match event {
        DevToolsProtocol::MethodResult {
            message_id,
            success,
            ..
        } if message_id == id => {
            let _ = ready.try_send(success);
        }
        DevToolsProtocol::Event { method, params } if method == "Fetch.requestPaused" => {
            let value: Value = serde_json::from_slice(&params).unwrap();
            requests
                .try_send((
                    value["requestId"].as_str().unwrap().to_owned(),
                    value["request"]["url"].as_str() == Some("https://avatar.example/self.png"),
                    value["request"]["url"]
                        .as_str()
                        .and_then(|url| tauri::Url::parse(url).ok())
                        .is_some_and(|url| {
                            url.scheme() == "http" && url.host_str() == Some("ipc.localhost")
                        }),
                ))
                .expect("Bounded synthetic requests");
        }
        _ => {}
    })
    .unwrap();
    let browser = view.clone();
    let html = html.to_owned();
    std::thread::spawn(move || {
        while let Ok((request, image, ipc)) = incoming.recv() {
            if ipc {
                let id = allocate_devtools_message_id().unwrap();
                browser.send_dev_tools_message(json!({"id":id,"method":"Fetch.continueRequest","params":{"requestId":request}}).to_string().as_bytes()).unwrap();
                continue;
            }
            let (mime, body) = if image {
                (
                    "image/png",
                    include_bytes!("../../icons/32x32.png").as_slice(),
                )
            } else {
                ("text/html", html.as_bytes())
            };
            let id = allocate_devtools_message_id().unwrap();
            if browser.send_dev_tools_message(json!({"id":id,"method":"Fetch.fulfillRequest","params":{"requestId":request,"responseCode":200,"responseHeaders":[{"name":"Content-Type","value":mime}],"body":gtk::glib::base64_encode(body).to_string()}}).to_string().as_bytes()).is_err() { break; }
        }
    });
    view.send_dev_tools_message(
        json!({"id":id,"method":"Fetch.enable","params":{"patterns":[{"urlPattern":"*"}]}})
            .to_string()
            .as_bytes(),
    )
    .unwrap();
    assert!(receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("Offline interception must be installed before navigation"));
}

#[track_caller]
pub fn wait_for(mut check: impl FnMut() -> bool) {
    for _ in 0..80 {
        if check() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("Synthetic condition did not become true");
}

//! Bounded queries in the main document's isolated JavaScript world.
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::{mpsc, Arc, Mutex},
    time::Duration,
};
use tauri_runtime_cef::{allocate_devtools_message_id, DevToolsProtocol, WebviewCefExt};
#[derive(Default)]
struct State {
    main_frame: Option<String>,
    context: Option<String>,
    pending: BTreeMap<i32, mpsc::SyncSender<Option<Value>>>,
}
#[derive(Default)]
pub struct Script {
    state: Mutex<State>,
}
impl Script {
    pub fn attach(view: &tauri::Webview) -> tauri::Result<Arc<Self>> {
        let script = Arc::new(Self::default());
        let observer = script.clone();
        view.on_dev_tools_protocol(move |event| {
            let mut state = observer.state.lock().unwrap();
            match event {
                DevToolsProtocol::Event { method, params } if params.len() <= 64 * 1024 => {
                    let Ok(value) = serde_json::from_slice::<Value>(&params) else {
                        return;
                    };
                    match method.as_str() {
                        "Page.frameNavigated" if value["frame"].get("parentId").is_none() => {
                            state.main_frame = value["frame"]["id"].as_str().map(str::to_owned);
                            state.context = None;
                            state.pending.clear();
                        }
                        "Runtime.executionContextsCleared" => {
                            state.context = None;
                            state.pending.clear();
                        }
                        "Runtime.executionContextDestroyed"
                            if value["executionContextUniqueId"].as_str()
                                == state.context.as_deref() =>
                        {
                            state.context = None;
                            state.pending.clear();
                        }
                        "Runtime.executionContextCreated" => {
                            let context = &value["context"];
                            if context["name"] == "elo-account-native"
                                && context["auxData"]["frameId"].as_str()
                                    == state.main_frame.as_deref()
                                && context["origin"]
                                    .as_str()
                                    .and_then(|origin| tauri::Url::parse(origin).ok())
                                    .is_some_and(|url| crate::whatsapp::is_origin(&url))
                            {
                                state.context = context["uniqueId"].as_str().map(str::to_owned);
                            }
                        }
                        _ => {}
                    }
                }
                DevToolsProtocol::MethodResult {
                    message_id,
                    success,
                    result,
                } => {
                    if let Some(sender) = state.pending.remove(&message_id) {
                        let value = if success && result.len() <= 128 * 1024 {
                            serde_json::from_slice::<Value>(&result)
                                .ok()
                                .and_then(|result| result["result"].get("value").cloned())
                        } else {
                            None
                        };
                        let _ = sender.try_send(value);
                    }
                }
                _ => {}
            }
        })?;
        for (method, params) in [
            ("Page.enable", json!({})),
            (
                "Page.addScriptToEvaluateOnNewDocument",
                json!({"worldName":"elo-account-native","source":"void 0"}),
            ),
        ] {
            let id = allocate_devtools_message_id().map_err(anyhow::Error::from)?;
            view.send_dev_tools_message(
                json!({"id":id,"method":method,"params":params})
                    .to_string()
                    .as_bytes(),
            )?;
        }
        Ok(script)
    }
    /// Call only on a worker thread. No page-provided expression is accepted.
    pub fn evaluate(&self, view: &tauri::Webview, expression: &'static str) -> Option<Value> {
        let id = allocate_devtools_message_id().ok()?;
        let (sender, receiver) = mpsc::sync_channel(1);
        let context = {
            let mut state = self.state.lock().unwrap();
            let context = state.context.clone()?;
            if state.pending.len() >= 2 {
                return None;
            }
            state.pending.insert(id, sender);
            context
        };
        let sent = view.send_dev_tools_message(json!({"id":id,"method":"Runtime.evaluate","params":{"expression":expression,"uniqueContextId":context,"returnByValue":true,"awaitPromise":false}}).to_string().as_bytes());
        let result = if sent.is_ok() {
            receiver.recv_timeout(Duration::from_secs(3)).ok().flatten()
        } else {
            None
        };
        self.state.lock().unwrap().pending.remove(&id);
        result
    }
}

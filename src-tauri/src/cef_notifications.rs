//! Browser-scoped notification transport. Native Chromium delivery stays blocked.
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tauri_runtime_cef::{allocate_devtools_message_id, DevToolsProtocol, WebviewCefExt};

fn command(
    view: &tauri::Webview,
    session: Option<&str>,
    method: &str,
    params: Value,
) -> tauri::Result<()> {
    let mut message = json!({"id": allocate_devtools_message_id().map_err(anyhow::Error::from)?, "method": method, "params": params});
    if let Some(session) = session {
        message = json!({"id": allocate_devtools_message_id().map_err(anyhow::Error::from)?, "method":"Target.sendMessageToTarget", "params":{"sessionId":session,"message":message.to_string()}});
    }
    view.send_dev_tools_message(&serde_json::to_vec(&message).expect("JSON command"))
}

pub fn attach(
    view: &tauri::Webview,
    origin: &str,
    notify: impl Fn(String, String) + Send + Sync + 'static,
    activity: impl Fn() + Send + Sync + 'static,
    failed: impl Fn() + Send + Sync + 'static,
) -> tauri::Result<()> {
    let origin = origin.to_owned();
    let script = include_str!("cef_notifications.js").replace(
        "__ELO_ACCOUNT_ORIGIN__",
        &serde_json::to_string(&origin).expect("Origin encoding"),
    );
    let worker_script = script.clone();
    let handle = view.clone();
    let contexts = Arc::new(Mutex::new(BTreeMap::<(String, i64), (bool, bool)>::new()));
    let (sender, receiver) = std::sync::mpsc::sync_channel(128);
    let overflow = Arc::new(AtomicBool::new(false));
    let overloaded = overflow.clone();
    let activity_script = r#"(() => { if (location.origin !== __ORIGIN__) return; const report = globalThis.__eloActivity; let last = 0; for (const name of ['keydown','pointerdown','pointermove','wheel']) addEventListener(name, event => { if (event.isTrusted && performance.now() - last >= 1000) { last = performance.now(); report(''); } }, {capture:true, passive:true}); })()"#.replace("__ORIGIN__", &serde_json::to_string(&origin).expect("Origin encoding"));
    let initialization = [
        ("Runtime.enable", json!({})),
        ("Runtime.addBinding", json!({"name":"__eloNotification"})),
        (
            "Runtime.addBinding",
            json!({"name":"__eloActivity","executionContextName":"elo-account-native"}),
        ),
        (
            "Page.addScriptToEvaluateOnNewDocument",
            json!({"worldName":"elo-account-native","source":activity_script}),
        ),
        (
            "Page.addScriptToEvaluateOnNewDocument",
            json!({"source":script}),
        ),
        (
            "Target.setAutoAttach",
            json!({"autoAttach":true,"waitForDebuggerOnStart":true,"flatten":false}),
        ),
    ]
    .into_iter()
    .map(|(method, params)| {
        allocate_devtools_message_id()
            .map(|id| (id, json!({"id":id,"method":method,"params":params})))
    })
    .collect::<Result<Vec<_>, _>>()
    .map_err(anyhow::Error::from)?;
    let outstanding = Arc::new(Mutex::new(
        initialization
            .iter()
            .map(|(id, _)| *id)
            .collect::<BTreeSet<_>>(),
    ));
    let initialized = outstanding.clone();
    view.on_dev_tools_protocol(move |event| {
        let completion = match &event {
            DevToolsProtocol::Message(bytes) if bytes.len() <= 64 * 1024 => {
                serde_json::from_slice::<Value>(bytes)
                    .ok()
                    .and_then(|value| {
                        value["id"]
                            .as_i64()
                            .and_then(|id| i32::try_from(id).ok())
                            .map(|id| (id, value.get("error").is_none()))
                    })
            }
            DevToolsProtocol::MethodResult {
                message_id,
                success,
                ..
            } => Some((*message_id, *success)),
            _ => None,
        };
        if let Some((id, success)) = completion {
            let mut pending = initialized.lock().unwrap();
            if pending.remove(&id) && (!success || pending.is_empty()) {
                let _ =
                    sender.try_send(json!({"method":"Elo.ready", "params":{"success":success}}));
            }
            return;
        }
        let DevToolsProtocol::Event { method, params } = event else {
            return;
        };
        if params.len() > 64 * 1024 {
            return;
        }
        let Ok(params) = serde_json::from_slice::<Value>(&params) else {
            return;
        };
        let mut message = json!({"method":method,"params":params});
        if message["method"] == "Target.receivedMessageFromTarget" {
            let session = message["params"]["sessionId"].clone();
            let Some(nested) = message["params"]["message"].as_str() else {
                return;
            };
            let Ok(mut nested) = serde_json::from_str::<Value>(nested) else {
                return;
            };
            nested["sessionId"] = session;
            message = nested;
        }
        if !matches!(
            message["method"].as_str(),
            Some(
                "Target.detachedFromTarget"
                    | "Debugger.paused"
                    | "Target.attachedToTarget"
                    | "Runtime.executionContextCreated"
                    | "Runtime.executionContextDestroyed"
                    | "Runtime.executionContextsCleared"
                    | "Runtime.bindingCalled"
            )
        ) {
            return;
        }
        if sender.try_send(message).is_err() {
            // Never block the CEF UI thread or print arbitrary page payloads.
            overloaded.store(true, Ordering::Relaxed);
        }
    })?;
    std::thread::spawn(move || {
        let mut workers = BTreeSet::new();
        let mut navigated = false;
        loop {
            if overflow.load(Ordering::Relaxed) {
                let _ = handle.close();
                failed();
                break;
            }
            let message = match receiver.recv_timeout(std::time::Duration::from_secs(10)) {
                Ok(message) => message,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) if navigated => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    let _ = handle.close();
                    failed();
                    break;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            };
            let session = message["sessionId"].as_str();
            let key = session.unwrap_or("").to_owned();
            let params = &message["params"];
            let result = match message["method"].as_str().unwrap_or("") {
                "Elo.ready" => {
                    if navigated {
                        continue;
                    }
                    navigated = true;
                    if params["success"] == true {
                        handle.navigate(origin.parse().expect("Validated account origin"))
                    } else {
                        failed();
                        handle.close()
                    }
                }
                "Target.detachedFromTarget" => {
                    if let Some(session) = params["sessionId"].as_str() {
                        workers.remove(session);
                        contexts
                            .lock()
                            .unwrap()
                            .retain(|(owner, _), _| owner != session);
                    }
                    Ok(())
                }
                "Target.attachedToTarget" => {
                    if let Some(worker) = params["sessionId"].as_str() {
                        if workers.len() < 64
                            && params["targetInfo"]["url"]
                                .as_str()
                                .and_then(|url| tauri::Url::parse(url).ok())
                                .is_some_and(|url| url.origin().ascii_serialization() == origin)
                        {
                            workers.insert(worker.to_owned());
                        }
                        let initialize = || -> tauri::Result<()> {
                            command(&handle, Some(worker), "Runtime.enable", json!({}))?;
                            command(
                                &handle,
                                Some(worker),
                                "Runtime.addBinding",
                                json!({"name":"__eloNotification"}),
                            )?;
                            command(&handle, Some(worker), "Debugger.enable", json!({}))?;
                            command(
                                &handle,
                                Some(worker),
                                "Debugger.setInstrumentationBreakpoint",
                                json!({"instrumentation":"beforeScriptExecution"}),
                            )?;
                            command(
                                &handle,
                                Some(worker),
                                "Runtime.runIfWaitingForDebugger",
                                json!({}),
                            )
                        };
                        initialize()
                    } else {
                        Ok(())
                    }
                }
                "Debugger.paused" if session.is_some() => {
                    let install = || -> tauri::Result<()> {
                        command(
                            &handle,
                            session,
                            "Runtime.evaluate",
                            json!({"expression":worker_script}),
                        )?;
                        command(&handle, session, "Debugger.disable", json!({}))
                    };
                    install()
                }
                "Runtime.executionContextCreated" => {
                    let context = &params["context"];
                    if let Some(id) = context["id"].as_i64() {
                        let trusted = context["origin"]
                            .as_str()
                            .and_then(|value| tauri::Url::parse(value).ok())
                            .is_some_and(|url| url.origin().ascii_serialization() == origin)
                            && (session.is_none() || workers.contains(&key));
                        let mut contexts = contexts.lock().unwrap();
                        if contexts.len() < 128 {
                            contexts.insert(
                                (key, id),
                                (trusted, context["name"] == "elo-account-native"),
                            );
                        }
                    }
                    Ok(())
                }
                "Runtime.executionContextDestroyed" => {
                    if let Some(id) = params["executionContextId"].as_i64() {
                        contexts.lock().unwrap().remove(&(key, id));
                    }
                    Ok(())
                }
                "Runtime.executionContextsCleared" => {
                    contexts
                        .lock()
                        .unwrap()
                        .retain(|(owner, _), _| owner != &key);
                    Ok(())
                }
                "Runtime.bindingCalled" if params["name"] == "__eloActivity" => {
                    if params["executionContextId"].as_i64().is_some_and(|id| {
                        contexts.lock().unwrap().get(&(key, id)) == Some(&(true, true))
                    }) {
                        activity();
                    }
                    Ok(())
                }
                "Runtime.bindingCalled" if params["name"] == "__eloNotification" => {
                    let trusted = params["executionContextId"].as_i64().is_some_and(|id| {
                        contexts
                            .lock()
                            .unwrap()
                            .get(&(key, id))
                            .is_some_and(|(trusted, _)| *trusted)
                    });
                    if trusted {
                        if let Some(payload) =
                            params["payload"].as_str().filter(|p| p.len() <= 8192)
                        {
                            if let Ok(value) = serde_json::from_str::<Value>(payload) {
                                if let (Some(title), Some(body)) =
                                    (value["title"].as_str(), value["body"].as_str())
                                {
                                    notify(
                                        title.chars().take(400).collect(),
                                        body.chars().take(1200).collect(),
                                    );
                                }
                            }
                        }
                    }
                    Ok(())
                }
                _ => Ok(()),
            };
            if result.is_err() {
                let _ = handle.close();
                failed();
                break;
            }
        }
    });
    for (_, message) in initialization {
        view.send_dev_tools_message(message.to_string().as_bytes())?;
    }
    Ok(())
}

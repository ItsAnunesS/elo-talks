//! Structural login-state changes in the main document's isolated world.
use crate::{
    account_events::{self, AccountEvents},
    account_runtime::{PageState, Readiness},
};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::Manager;
use tauri_runtime_cef::{allocate_devtools_message_id, DevToolsProtocol, WebviewCefExt};

fn source() -> &'static str {
    static SOURCE: OnceLock<String> = OnceLock::new();
    SOURCE.get_or_init(|| {
        include_str!("readiness_watch.js")
            .replace("__READINESS_QUERY__", include_str!("readiness.js"))
    })
}
#[derive(Default)]
struct Admission {
    frame: Option<String>,
    context: Option<i64>,
    epoch: u64,
}
impl Admission {
    fn invalidate(&mut self) {
        self.context = None;
        self.epoch = self.epoch.saturating_add(1);
    }
    fn event(&mut self, method: &str, value: &Value) -> Option<Readiness> {
        match method {
            "Page.frameNavigated" if value["frame"].get("parentId").is_none() => {
                self.frame = value["frame"]["id"].as_str().map(str::to_owned);
                self.invalidate();
            }
            "Page.frameStartedLoading" if value["frameId"].as_str() == self.frame.as_deref() => {
                self.invalidate()
            }
            "Runtime.executionContextsCleared" => self.invalidate(),
            "Runtime.executionContextDestroyed"
                if value["executionContextId"].as_i64() == self.context =>
            {
                self.invalidate()
            }
            "Runtime.executionContextCreated" => {
                let context = &value["context"];
                if context["name"] == "elo-account-native"
                    && self.frame.is_some()
                    && context["auxData"]["frameId"].as_str() == self.frame.as_deref()
                    && context["origin"]
                        .as_str()
                        .and_then(|origin| tauri::Url::parse(origin).ok())
                        .is_some_and(|url| crate::whatsapp::is_origin(&url))
                {
                    let context = context["id"].as_i64();
                    if self.context != context {
                        self.context = context;
                        self.epoch = self.epoch.saturating_add(1);
                    }
                }
            }
            "Runtime.bindingCalled"
                if value["name"] == "__eloReadiness"
                    && self.context.is_some()
                    && value["executionContextId"].as_i64() == self.context =>
            {
                return match value["payload"].as_str() {
                    Some("0") => Some(Readiness::Preparing),
                    Some("1") => Some(Readiness::Chats),
                    Some("2") => Some(Readiness::QrCode),
                    _ => None,
                };
            }
            _ => {}
        }
        None
    }
}

/// Register before the first account navigation. The page receives no native commands.
pub fn observe(view: &tauri::Webview, app: &tauri::AppHandle, id: &str) -> tauri::Result<()> {
    let admission = Arc::new(Mutex::new(Admission::default()));
    let handle = app.clone();
    let account = id.to_owned();
    let account_view = view.clone();
    view.on_dev_tools_protocol(move |event| {
        let DevToolsProtocol::Event { method, params } = event else {
            return;
        };
        if params.len() > 64 * 1024 {
            return;
        }
        if !matches!(
            method.as_str(),
            "Page.frameNavigated"
                | "Page.frameStartedLoading"
                | "Runtime.executionContextCreated"
                | "Runtime.executionContextDestroyed"
                | "Runtime.executionContextsCleared"
                | "Runtime.bindingCalled"
        ) {
            return;
        }
        let Ok(value) = serde_json::from_slice::<Value>(&params) else {
            return;
        };
        let (readiness, epoch) = {
            let mut admission = admission.lock().unwrap();
            let Some(readiness) = admission.event(&method, &value) else {
                return;
            };
            (readiness, admission.epoch)
        };
        let handle = handle.clone();
        let queued = handle.clone();
        let account = account.clone();
        let admission = admission.clone();
        let view = account_view.clone();
        let _ =
            handle.run_on_main_thread(move || {
                if admission.lock().unwrap().epoch != epoch {
                    return;
                }
                {
                    let events = queued.state::<AccountEvents>();
                    let mut runtime = events.runtime.lock().unwrap();
                    let Some(health) = runtime.accounts.get_mut(&account).filter(|health| {
                        matches!(health.page, PageState::Loading | PageState::Ready)
                    }) else {
                        return;
                    };
                    if health.readiness == readiness {
                        return;
                    }
                    health.readiness = readiness;
                }
                if readiness == Readiness::QrCode {
                    crate::account_avatar::clear(&queued, &account);
                }
                if readiness == Readiness::Chats {
                    crate::account_avatar::attach(&view, &queued, &account);
                }
                account_events::publish(&queued);
            });
    })?;
    for (method, params) in [
        (
            "Runtime.addBinding",
            json!({"name":"__eloReadiness","executionContextName":"elo-account-native"}),
        ),
        (
            "Page.addScriptToEvaluateOnNewDocument",
            json!({"worldName":"elo-account-native","source":source()}),
        ),
    ] {
        let id = allocate_devtools_message_id().map_err(anyhow::Error::from)?;
        view.send_dev_tools_message(
            json!({"id":id,"method":method,"params":params})
                .to_string()
                .as_bytes(),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn readiness_reports_require_the_admitted_main_frame_world() {
        let mut admission = Admission::default();
        let report = json!({"name":"__eloReadiness","executionContextId":7,"payload":"1"});
        assert!(admission.event("Runtime.bindingCalled", &report).is_none());
        admission.event("Page.frameNavigated", &json!({"frame":{"id":"main"}}));
        for context in [
            json!({"id":7,"name":"", "origin":"https://web.whatsapp.com", "auxData":{"frameId":"main"}}),
            json!({"id":7,"name":"elo-account-native", "origin":"https://untrusted.example", "auxData":{"frameId":"main"}}),
            json!({"id":7,"name":"elo-account-native", "origin":"https://web.whatsapp.com", "auxData":{"frameId":"child"}}),
        ] {
            admission.event(
                "Runtime.executionContextCreated",
                &json!({"context":context}),
            );
            assert!(admission.event("Runtime.bindingCalled", &report).is_none());
        }
        admission.event("Runtime.executionContextCreated", &json!({"context":{"id":7,"name":"elo-account-native", "origin":"https://web.whatsapp.com", "auxData":{"frameId":"main"}}}));
        for (payload, expected) in [
            ("2", Readiness::QrCode),
            ("1", Readiness::Chats),
            ("0", Readiness::Preparing),
        ] {
            let mut report = report.clone();
            report["payload"] = json!(payload);
            assert_eq!(
                admission.event("Runtime.bindingCalled", &report),
                Some(expected)
            );
        }
        assert!(admission
            .event(
                "Runtime.bindingCalled",
                &json!({"name":"__eloReadiness","executionContextId":8,"payload":"1"})
            )
            .is_none());
        assert!(admission.event("Runtime.bindingCalled", &json!({"name":"__eloReadiness","executionContextId":7,"payload":"{private:content}"})).is_none());
        let epoch = admission.epoch;
        admission.event("Runtime.executionContextsCleared", &json!({}));
        assert!(admission.epoch > epoch);
        assert!(admission.event("Runtime.bindingCalled", &report).is_none());
    }
}

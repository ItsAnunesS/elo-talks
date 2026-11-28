//! Optional bounded photo resolution in the admitted browser document, never native URL downloads.
use crate::{account_script::Script, notification_content::NotificationImage};
use serde_json::json;

pub fn resolve(view: &tauri::Webview, script: &Script, source: &str) -> Option<NotificationImage> {
    if source.len() > 4096 {
        return None;
    }
    let function = script.evaluate_remote(view, include_str!("notification_photo.js"), false)?;
    let object = function["result"]["objectId"].as_str()?;
    let result = script.command(
        view,
        "Runtime.callFunctionOn",
        json!({
            "objectId":object,"functionDeclaration":"function (source) { return this(source); }",
            "arguments":[{"value":source}],"awaitPromise":true,"returnByValue":true
        }),
    );
    let _ = script.command(view, "Runtime.releaseObject", json!({"objectId":object}));
    NotificationImage::from_data_url(result?["result"]["value"].as_str()?)
}

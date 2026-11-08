//! Presentation only; no conversation data is read.
pub fn attach(view: &tauri::Webview) {
    let css = serde_json::to_string(include_str!("whatsapp.css")).expect("CSS encoding");
    let _ = view.eval(format!("(() => {{ if (location.origin !== 'https://web.whatsapp.com' || document.getElementById('elo-theme')) return; const style = document.createElement('style'); style.id = 'elo-theme'; style.textContent = {css}; document.head.append(style); }})()"));
}

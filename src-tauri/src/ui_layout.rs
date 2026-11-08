//! CEF child bounds keep remote content below the trusted shell controls.
use tauri::{LogicalPosition, LogicalSize, Manager};
pub const SIDEBAR_WIDTH: i32 = 304;
pub const HEADER_HEIGHT: i32 = 108;
pub const RESIZE_BORDER: f64 = 6.;
pub fn attach(window: &tauri::Window, shell: &tauri::Webview) -> tauri::Result<()> {
    let handle = window.clone();
    shell.set_size(window.inner_size()?)?;
    let shell = shell.clone();
    window.on_window_event(move |event| {
        if matches!(
            event,
            tauri::WindowEvent::Resized(_) | tauri::WindowEvent::ScaleFactorChanged { .. }
        ) {
            let Ok(size) = handle.inner_size() else {
                return;
            };
            let Ok(scale) = handle.scale_factor() else {
                return;
            };
            let size = size.to_logical::<f64>(scale);
            let _ = shell.set_size(size);
            for (label, view) in handle.app_handle().webviews() {
                if label.starts_with("account-") {
                    let _ = view.set_position(LogicalPosition::new(
                        SIDEBAR_WIDTH as f64,
                        HEADER_HEIGHT as f64,
                    ));
                    let _ = view.set_size(LogicalSize::new(
                        (size.width - SIDEBAR_WIDTH as f64 - RESIZE_BORDER).max(1.),
                        (size.height - HEADER_HEIGHT as f64 - RESIZE_BORDER).max(1.),
                    ));
                }
            }
        }
    });
    Ok(())
}
pub fn set_account_visible(view: &tauri::Webview, visible: bool) -> tauri::Result<()> {
    if visible {
        view.show()?;
        view.set_focus()
    } else {
        view.hide()
    }
}

//! Window geometry contains no account data and is scoped to the UI profile.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Serialize, Deserialize)]
pub struct Geometry {
    pub width: i32,
    pub height: i32,
    pub maximized: bool,
}
impl Default for Geometry {
    fn default() -> Self {
        Self {
            width: 1200,
            height: 800,
            maximized: false,
        }
    }
}
pub fn load(profile: &Path) -> Geometry {
    crate::storage::read::<Geometry>(&profile.join("window.json"))
        .ok()
        .filter(|value| (900..=7680).contains(&value.width) && (640..=4320).contains(&value.height))
        .unwrap_or_default()
}
pub fn attach(window: &tauri::Window, path: PathBuf, saved: Geometry) {
    let geometry = std::sync::Arc::new(std::sync::Mutex::new(saved));
    let pending = std::sync::Arc::new(std::sync::Mutex::new(None::<gtk::glib::SourceId>));
    let handle = window.clone();
    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Resized(_)) {
            let mut value = geometry.lock().unwrap();
            value.maximized = handle.is_maximized().unwrap_or(false);
            if !value.maximized {
                if let (Ok(size), Ok(scale)) = (handle.inner_size(), handle.scale_factor()) {
                    let size = size.to_logical::<i32>(scale);
                    if size.width >= 900 && size.height >= 640 {
                        value.width = size.width;
                        value.height = size.height;
                    }
                }
            }
            drop(value);
            if let Some(timer) = pending.lock().unwrap().take() {
                timer.remove();
            }
            let snapshot = geometry.clone();
            let destination = path.clone();
            let completion = pending.clone();
            let timer = gtk::glib::timeout_add_local_once(
                std::time::Duration::from_millis(350),
                move || {
                    completion.lock().unwrap().take();
                    let _ = crate::storage::write(&destination, &*snapshot.lock().unwrap());
                },
            );
            *pending.lock().unwrap() = Some(timer);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn geometry_roundtrip_and_invalid_bounds() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(load(root.path()).width, 1200);
        crate::storage::write(
            &root.path().join("window.json"),
            &Geometry {
                width: 1440,
                height: 900,
                maximized: true,
            },
        )
        .unwrap();
        let restored = load(root.path());
        assert_eq!(
            (restored.width, restored.height, restored.maximized),
            (1440, 900, true)
        );
        crate::storage::write(
            &root.path().join("window.json"),
            &Geometry {
                width: 1,
                height: 900,
                maximized: true,
            },
        )
        .unwrap();
        assert_eq!(load(root.path()).width, 1200);
    }
}

//! Window geometry contains no account data and is scoped to the UI profile.
use gtk::{glib, prelude::*};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
    time::Duration,
};
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
pub fn attach(window: &gtk::Window, path: PathBuf, saved: Geometry) {
    let geometry = Rc::new(RefCell::new(saved));
    let pending = Rc::new(RefCell::new(None::<glib::SourceId>));
    window.connect_configure_event(move |window, _| {
        let maximized = window.is_maximized();
        if !maximized {
            let (width, height) = window.size();
            if width >= 900 && height >= 640 {
                geometry.borrow_mut().width = width;
                geometry.borrow_mut().height = height;
            }
        }
        geometry.borrow_mut().maximized = maximized;
        if let Some(timer) = pending.borrow_mut().take() {
            timer.remove();
        }
        let completion = pending.clone();
        let geometry = geometry.clone();
        let path = path.clone();
        let timer = glib::timeout_add_local_once(Duration::from_millis(350), move || {
            completion.borrow_mut().take();
            let _ = crate::storage::write(&path, &*geometry.borrow());
        });
        *pending.borrow_mut() = Some(timer);
        false
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

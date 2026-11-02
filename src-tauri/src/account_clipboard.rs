//! User-initiated PNG paste for WebKitGTK versions that omit clipboard files.
use gtk::{gdk, gio, glib, prelude::*};
use javascriptcore::ValueExt;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Instant,
};
use webkit2gtk::{ContextMenuExt, ContextMenuItemExt, WebViewExt};

const WORLD: &str = "elo-clipboard";
const MAX_PNG_BYTES: usize = 20 * 1024 * 1024;

#[derive(Default)]
struct Paste {
    generation: Cell<u64>,
    pending: Cell<bool>,
    operation: RefCell<Option<gio::Cancellable>>,
}

fn eligible(view: &webkit2gtk::WebView) -> bool {
    view.is_visible()
        && view.has_focus()
        && view
            .uri()
            .and_then(|uri| tauri::Url::parse(&uri).ok())
            .is_some_and(|url| crate::whatsapp::is_origin(&url))
}

impl Paste {
    fn cancel(&self) {
        self.generation.set(self.generation.get().wrapping_add(1));
        self.pending.set(false);
        if let Some(operation) = self.operation.borrow_mut().take() {
            operation.cancel();
        }
    }

    fn begin(self: &Rc<Self>, view: &webkit2gtk::WebView, error: Rc<dyn Fn(&str)>) {
        if !eligible(view) || self.pending.replace(true) {
            return;
        }
        let generation = self.generation.get();
        let started = Instant::now();
        let state = self.clone();
        let weak = view.downgrade();
        let operation = gio::Cancellable::new();
        *self.operation.borrow_mut() = Some(operation.clone());
        let delivery = operation.clone();
        // A deadline also releases the in-flight guard if the clipboard owner stalls.
        let deadline = self.clone();
        let timeout_error = error.clone();
        glib::timeout_add_local_once(std::time::Duration::from_secs(3), move || {
            if deadline.generation.get() == generation && deadline.pending.get() {
                deadline.cancel();
                timeout_error("Pasting timed out. Copy the image again and retry.");
            }
        });
        let script = format!(
            "(() => {{ const target = document.activeElement; globalThis.eloPaste = null; if (location.origin !== 'https://web.whatsapp.com' || !document.hasFocus() || !target?.isContentEditable) return false; globalThis.eloPaste = {{target, generation: {generation}}}; return true; }})()"
        );
        view.evaluate_javascript(&script, Some(WORLD), None, Some(&operation), move |result| {
            let Some(view) = weak.upgrade() else { return };
            if state.generation.get() != generation {
                return;
            }
            if !eligible(&view) {
                state.cancel();
                return;
            }
            if !result.is_ok_and(|value| value.to_boolean()) {
                state.cancel();
                view.execute_editing_command("Paste");
                return;
            }
            let weak = view.downgrade();
            gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD).request_contents(&gdk::Atom::intern("image/png"), move |_, selection| {
                let Some(view) = weak.upgrade() else { return };
                if state.generation.get() != generation {
                    return;
                }
                if started.elapsed().as_secs() >= 3 || !eligible(&view) {
                    state.cancel();
                    return;
                }
                if selection.length() <= 0 {
                    state.cancel();
                    view.execute_editing_command("Paste");
                    return;
                }
                if selection.length() as usize > MAX_PNG_BYTES {
                    state.cancel();
                    error("This clipboard image is too large. Use an image smaller than 20 MB.");
                    return;
                }
                let bytes = selection.data();
                if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
                    state.cancel();
                    // Do not inject malformed or unbounded clipboard data.
                    error("This clipboard image could not be pasted. Use a PNG image smaller than 20 MB.");
                    return;
                }
                let script = include_str!("clipboard.js")
                    .replace("__GENERATION__", &generation.to_string())
                    .replace("__PNG__", &glib::base64_encode(&bytes));
                view.evaluate_javascript(&script, Some(WORLD), None, Some(&delivery), move |result| {
                    if state.generation.get() == generation {
                        state.cancel();
                        if result.is_err() {
                            error("The image could not be pasted. Copy it again and retry.");
                        }
                    }
                });
            });
        });
    }
}

pub fn attach(view: &webkit2gtk::WebView, error: impl Fn(&str) + 'static) {
    let state = Rc::new(Paste::default());
    let error: Rc<dyn Fn(&str)> = Rc::new(error);
    let keys = state.clone();
    let key_error = error.clone();
    view.connect_key_press_event(move |view, event| {
        let modifiers = event.state() & gtk::accelerator_get_default_mod_mask();
        let paste = (modifiers == gdk::ModifierType::CONTROL_MASK
            && matches!(
                event.keyval(),
                gdk::keys::constants::v | gdk::keys::constants::V
            ))
            || (modifiers == gdk::ModifierType::SHIFT_MASK
                && event.keyval() == gdk::keys::constants::Insert);
        if paste && eligible(view) {
            keys.begin(view, key_error.clone());
            glib::Propagation::Stop
        } else {
            keys.cancel();
            glib::Propagation::Proceed
        }
    });
    let menu_state = state.clone();
    view.connect_context_menu(move |view, menu, _, _| {
        for (index, item) in menu.items().iter().enumerate() {
            if item.stock_action() != webkit2gtk::ContextMenuAction::Paste {
                continue;
            }
            let action = gio::SimpleAction::new("elo-paste", None);
            let weak = view.downgrade();
            let state = menu_state.clone();
            let error = error.clone();
            action.connect_activate(move |_, _| {
                if let Some(view) = weak.upgrade() {
                    state.begin(&view, error.clone());
                }
            });
            menu.remove(item);
            menu.insert(
                &webkit2gtk::ContextMenuItem::from_gaction(&action, "Paste", None),
                index as i32,
            );
        }
        false
    });
    let focus = state.clone();
    let pointer = state.clone();
    view.connect_button_press_event(move |_, _| {
        pointer.cancel();
        glib::Propagation::Proceed
    });
    view.connect_focus_out_event(move |_, _| {
        focus.cancel();
        glib::Propagation::Proceed
    });
    let hidden = state.clone();
    view.connect_hide(move |_| hidden.cancel());
    view.connect_load_changed(move |_, event| {
        if event == webkit2gtk::LoadEvent::Started {
            state.cancel();
        }
    });
}

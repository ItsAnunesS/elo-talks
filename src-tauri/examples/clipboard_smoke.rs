//! Synthetic clipboard only; run in an isolated Xvfb and D-Bus session.
#[path = "../src/account_clipboard.rs"]
mod account_clipboard;
#[allow(dead_code)]
#[path = "../src/whatsapp.rs"]
mod whatsapp;
use gtk::{glib, prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};
use webkit2gtk::{WebContextExt, WebViewExt};

static STAGE: AtomicUsize = AtomicUsize::new(0);
const STAGES: [&str; 9] = [
    "GTK initialization",
    "WebKit initialization",
    "document loading",
    "image paste",
    "text paste",
    "invalid image",
    "hidden account cancellation",
    "untrusted origin",
    "finished",
];
fn stage(index: usize) {
    STAGE.store(index, Ordering::Relaxed);
    println!("Clipboard fixture: {}", STAGES[index]);
}

fn paste(key: &'static str) {
    // Send real X11 events from a separate process. GTK's deprecated synchronous
    // key helper can deadlock WebKit clipboard IPC while the main loop is busy.
    // Search only this synthetic fixture's windows on its private Xvfb display.
    std::thread::spawn(move || {
        let result = std::process::Command::new("xdotool")
            .args([
                "search",
                "--sync",
                "--onlyvisible",
                "--all",
                "--pid",
                &std::process::id().to_string(),
                "--name",
                ".",
                "windowfocus",
                "--sync",
                "mousemove",
                "--window",
                "%1",
                "20",
                "22",
                "click",
                "1",
                "key",
                "--clearmodifiers",
                key,
                "key",
                "Shift_L",
            ])
            .status();
        if !result.is_ok_and(|status| status.success()) {
            eprintln!("::error::Could not send a synthetic clipboard shortcut with xdotool");
            std::process::exit(1);
        }
    });
}

fn main() {
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(25));
        eprintln!(
            "::error::Clipboard fixture stalled during {}",
            STAGES[STAGE.load(Ordering::Relaxed)]
        );
        std::process::exit(1);
    });
    gtk::init().unwrap();
    stage(1);
    let context = webkit2gtk::WebContext::new_ephemeral();
    context.set_sandbox_enabled(true);
    assert!(context.is_sandbox_enabled());
    let view = webkit2gtk::WebView::with_context(&context);
    let key_events = Rc::new(Cell::new(0usize));
    let observed_keys = key_events.clone();
    view.connect_key_press_event(move |_, _| {
        observed_keys.set(observed_keys.get() + 1);
        glib::Propagation::Proceed
    });
    let native = std::env::args().any(|argument| argument == "--native");
    let errors = Rc::new(RefCell::new(Vec::new()));
    if !native {
        let errors = errors.clone();
        account_clipboard::attach(&view, move |error| {
            errors.borrow_mut().push(error.to_owned())
        });
    }
    let image = gtk::gdk_pixbuf::Pixbuf::from_read(std::io::Cursor::new(include_bytes!(
        "../icons/32x32.png"
    )))
    .unwrap();
    let clipboard = gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD);
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("Elo Talks — Clipboard fixture");
    window.set_default_size(480, 320);
    window.add(&view);
    window.show_all();
    window.present();
    stage(2);
    view.load_html(
        "<div contenteditable id='editor' style='min-height:48px'>Synthetic editor</div><script>editor.focus(); let count=0; let keys=0; const report=()=>{if(count===0)document.title='ready|trusted:'+(location.origin==='https://web.whatsapp.com')+'|focus:'+document.hasFocus()+'|editable:'+!!document.activeElement?.isContentEditable+'|keys:'+keys;}; document.addEventListener('keydown',()=>{keys++;report();}); document.addEventListener('focusin',report); window.addEventListener('focus',report); document.addEventListener('click',report); document.addEventListener('paste', async e => { e.preventDefault(); count++; document.title='paste-received:' + count; try { const file=e.clipboardData.files[0]; document.title = file ? 'image:' + file.type + ':' + ((bytes) => bytes.length > 8 && bytes[0] === 137 && bytes[1] === 80)(new Uint8Array(await file.arrayBuffer())) + ':' + count : 'text:' + e.clipboardData.getData('text/plain') + ':' + count; } catch (error) {document.title='paste-error:' + error.name;} }); report();</script>",
        Some("https://web.whatsapp.com/"),
    );
    let mut changed = Instant::now();
    let mut untrusted_paste = false;
    // Clipboard ownership can invoke nested GTK dispatch. Regular timeout sources
    // are non-reentrant; do not perform these operations inside a polled future.
    glib::timeout_add_local(Duration::from_millis(100), move || {
        let phase = STAGE.load(Ordering::Relaxed);
        let ready = match phase {
            2 => view
                .title()
                .is_some_and(|title| title.starts_with("ready|")),
            3 if native => changed.elapsed() >= Duration::from_secs(1),
            3 => view.title().as_deref() == Some("image:image/png:true:1"),
            4 => view.title().as_deref() == Some("text:Synthetic clipboard text:2"),
            5 => !errors.borrow().is_empty(),
            6 => !view.is_visible(),
            7 if untrusted_paste => changed.elapsed() >= Duration::from_secs(1),
            7 => view.title().as_deref() == Some("untrusted-ready"),
            _ => unreachable!(),
        };
        if !ready {
            let limit = if phase == 2 { 5 } else { 3 };
            if changed.elapsed() >= Duration::from_secs(limit) {
                // All content belongs to this synthetic document; never run this
                // diagnostic against a live account or a user's clipboard.
                let title: String = view.title().unwrap_or_default().chars().take(100).collect();
                let native_origin = view
                    .uri()
                    .and_then(|uri| tauri::Url::parse(&uri).ok())
                    .is_some_and(|uri| whatsapp::is_origin(&uri));
                eprintln!("::error::Clipboard assertion during {}: title={title:?}, native_origin={native_origin}, focused={}, visible={}, keys={}, errors={:?}", STAGES[phase], view.has_focus(), view.is_visible(), key_events.get(), errors.borrow());
                std::process::exit(1);
            }
            return glib::ControlFlow::Continue;
        }
        changed = Instant::now();
        match phase {
            2 => {
                view.grab_focus();
                stage(3);
                clipboard.set_image(&image);
                if native {
                    view.execute_editing_command("Paste");
                } else {
                    // Keep the web process busy while real X11 events send the
                    // paste shortcut followed by a modifier-only key. The paste
                    // must survive that key while its target capture is pending.
                    view.evaluate_javascript(
                        "(() => { const until = performance.now() + 250; while (performance.now() < until) {} })()",
                        None,
                        None,
                        gtk::gio::Cancellable::NONE,
                        |_| {},
                    );
                    paste("ctrl+v");
                }
            }
            3 if native => {
                println!("Native image paste: {:?}", view.title());
                std::process::exit(0);
            }
            3 => {
                assert_eq!(view.title().as_deref(), Some("image:image/png:true:1"));
                stage(4);
                clipboard.set_text("Synthetic clipboard text");
                paste("shift+Insert");
            }
            4 => {
                assert_eq!(
                    view.title().as_deref(),
                    Some("text:Synthetic clipboard text:2")
                );
                assert!(errors.borrow().is_empty());
                stage(5);
                let target = gtk::TargetEntry::new("image/png", gtk::TargetFlags::empty(), 0);
                assert!(clipboard.set_with_data(&[target], |_, selection, _| {
                    selection.set(
                        &gtk::gdk::Atom::intern("image/png"),
                        8,
                        b"Invalid synthetic PNG",
                    );
                }));
                paste("ctrl+v");
            }
            5 => {
                assert_eq!(
                    errors.borrow().len(),
                    1,
                    "Invalid images must report an error"
                );
                assert_eq!(
                    view.title().as_deref(),
                    Some("text:Synthetic clipboard text:2")
                );
                stage(6);
                let hidden = view.clone();
                let target = gtk::TargetEntry::new("image/png", gtk::TargetFlags::empty(), 0);
                assert!(clipboard.set_with_data(&[target], move |_, selection, _| {
                    hidden.hide();
                    selection.set(
                        &gtk::gdk::Atom::intern("image/png"),
                        8,
                        include_bytes!("../icons/32x32.png"),
                    );
                }));
                paste("ctrl+v");
            }
            6 => {
                assert!(!view.is_visible());
                assert_eq!(
                    view.title().as_deref(),
                    Some("text:Synthetic clipboard text:2"),
                    "Switching away must cancel an in-flight image paste"
                );
                assert_eq!(errors.borrow().len(), 1);
                stage(7);
                view.show();
                view.grab_focus();
                view.load_html("<div contenteditable id='editor' style='min-height:48px'></div><script>editor.focus(); document.addEventListener('paste', e => {e.preventDefault(); document.title=String(e.isTrusted);}); document.title='untrusted-ready';</script>", Some("https://untrusted.example/"));
            }
            7 if !untrusted_paste => {
                untrusted_paste = true;
                clipboard.set_image(&image);
                paste("ctrl+v");
            }
            7 => {
                assert_ne!(view.title().as_deref(), Some("false"));
                assert_eq!(errors.borrow().len(), 1);
                stage(8);
                println!("PASS: PNG paste with an in-flight modifier, text fallback, invalid image rejection, hidden-account cancellation and origin restriction");
                // This standalone fixture tests paste, not WebKit teardown. Match
                // layout_smoke: exit only after every behavioral assertion passed.
                std::process::exit(0);
            }
            _ => unreachable!(),
        }
        glib::ControlFlow::Continue
    });
    gtk::main();
}

//! Synthetic clipboard only; run in an isolated Xvfb and D-Bus session.
#[path = "../src/account_clipboard.rs"]
mod account_clipboard;
#[allow(dead_code)]
#[path = "../src/whatsapp.rs"]
mod whatsapp;
use gtk::{glib, prelude::*};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
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
                "--pid",
                &std::process::id().to_string(),
                "windowfocus",
                "--sync",
                "key",
                "--clearmodifiers",
                key,
            ])
            .status();
        if !result.is_ok_and(|status| status.success()) {
            eprintln!("::error::Could not send a synthetic clipboard shortcut with xdotool");
            std::process::exit(1);
        }
    });
}

async fn settle(check: impl Fn() -> bool) {
    for _ in 0..30 {
        if check() {
            return;
        }
        glib::timeout_future(Duration::from_millis(100)).await;
    }
    panic!(
        "Clipboard assertion did not settle during {}",
        STAGES[STAGE.load(Ordering::Relaxed)]
    );
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
    let native = std::env::args().any(|argument| argument == "--native");
    let errors = Rc::new(RefCell::new(Vec::new()));
    if !native {
        let errors = errors.clone();
        account_clipboard::attach(&view, move |error| {
            errors.borrow_mut().push(error.to_owned())
        });
    }
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.add(&view);
    window.show_all();
    window.present();
    stage(2);
    view.load_html(
        "<div contenteditable id='editor'>Synthetic editor</div><script>editor.focus(); let count=0; document.addEventListener('paste', async e => { e.preventDefault(); count++; const file=e.clipboardData.files[0]; document.title = file ? 'image:' + file.type + ':' + ((bytes) => bytes.length > 8 && bytes[0] === 137 && bytes[1] === 80)(new Uint8Array(await file.arrayBuffer())) + ':' + count : 'text:' + e.clipboardData.getData('text/plain') + ':' + count; });</script>",
        Some("https://web.whatsapp.com/"),
    );
    glib::MainContext::default().spawn_local(async move {
        glib::timeout_future(Duration::from_secs(2)).await;
        view.grab_focus();
        let clipboard = gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD);
        let image = gtk::gdk_pixbuf::Pixbuf::from_read(std::io::Cursor::new(include_bytes!("../icons/32x32.png"))).unwrap();
        stage(3);
        clipboard.set_image(&image);
        if native {
            view.execute_editing_command("Paste");
        } else {
            paste("ctrl+v");
        }
        if native {
            glib::timeout_future(Duration::from_secs(1)).await;
            println!("Native image paste: {:?}", view.title());
        } else {
            settle(|| view.title().as_deref() == Some("image:image/png:true:1")).await;
            assert_eq!(view.title().as_deref(), Some("image:image/png:true:1"));
            stage(4);
            clipboard.set_text("Synthetic clipboard text");
            paste("shift+Insert");
            settle(|| view.title().as_deref() == Some("text:Synthetic clipboard text:2")).await;
            assert_eq!(view.title().as_deref(), Some("text:Synthetic clipboard text:2"));
            assert!(errors.borrow().is_empty());
            stage(5);
            let target = gtk::TargetEntry::new("image/png", gtk::TargetFlags::empty(), 0);
            assert!(clipboard.set_with_data(std::slice::from_ref(&target), |_, selection, _| {
                selection.set(&gtk::gdk::Atom::intern("image/png"), 8, b"Invalid synthetic PNG");
            }));
            paste("ctrl+v");
            settle(|| errors.borrow().len() == 1).await;
            assert_eq!(errors.borrow().len(), 1, "Invalid images must report an error");
            assert_eq!(view.title().as_deref(), Some("text:Synthetic clipboard text:2"));
            stage(6);
            let hidden = view.clone();
            assert!(clipboard.set_with_data(&[target], move |_, selection, _| {
                hidden.hide();
                selection.set(&gtk::gdk::Atom::intern("image/png"), 8, include_bytes!("../icons/32x32.png"));
            }));
            paste("ctrl+v");
            settle(|| !view.is_visible()).await;
            assert!(!view.is_visible());
            assert_eq!(view.title().as_deref(), Some("text:Synthetic clipboard text:2"), "Switching away must cancel an in-flight image paste");
            assert_eq!(errors.borrow().len(), 1);
            view.show();
            view.grab_focus();
            stage(7);
            view.load_html("<div contenteditable id='editor'></div><script>editor.focus(); document.addEventListener('paste', e => {e.preventDefault(); document.title=String(e.isTrusted);});</script>", Some("https://untrusted.example/"));
            glib::timeout_future(Duration::from_secs(1)).await;
            clipboard.set_image(&image);
            paste("ctrl+v");
            glib::timeout_future(Duration::from_secs(1)).await;
            assert_ne!(view.title().as_deref(), Some("false"));
            println!("PASS: PNG paste, text fallback, invalid image rejection, hidden-account cancellation and origin restriction");
        }
        stage(8);
        gtk::main_quit();
    });
    gtk::main();
}

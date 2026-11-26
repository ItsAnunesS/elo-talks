//! Offline cross-process persistence using the production browser policy.
#[path = "../src/account_browser.rs"]
mod account_browser;
#[allow(dead_code)]
#[path = "../src/account_profiles.rs"]
mod account_profiles;
#[path = "support/cef_fixture.rs"]
mod fixture;
#[path = "../src/process_shutdown.rs"]
mod process_shutdown;
#[allow(dead_code)]
#[path = "../src/storage.rs"]
mod storage;
use std::{
    os::unix::process::CommandExt,
    path::Path,
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tauri::{webview::WebviewBuilder, LogicalSize, WebviewUrl};
use tauri_runtime_cef::SecretStorage;

const ACCOUNT: &str = "11111111-1111-4111-8111-111111111111";
const OTHER: &str = "22222222-2222-4222-8222-222222222222";
const HTML: &str = r#"<!doctype html><body>Synthetic persistence fixture<script>
window.checkSession = async (write, isolated) => {
  const database = await new Promise((resolve, reject) => {
    const request = indexedDB.open('synthetic-session', 1);
    request.onupgradeneeded = () => request.result.createObjectStore('keys');
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(Error('Synthetic database open failed'));
  });
  if (write) {
    document.cookie = 'syntheticSession=preserve; Secure; SameSite=Strict; Path=/';
    document.cookie = 'syntheticPersistent=preserve; Secure; SameSite=Strict; Path=/; Max-Age=3600';
    localStorage.setItem('synthetic-session', 'preserve');
    const key = await crypto.subtle.generateKey({name:'AES-GCM',length:256}, false, ['encrypt','decrypt']);
    const iv = crypto.getRandomValues(new Uint8Array(12));
    const encrypted = await crypto.subtle.encrypt({name:'AES-GCM',iv}, key, new TextEncoder().encode('synthetic login'));
    await new Promise((resolve, reject) => {
      const transaction = database.transaction('keys', 'readwrite');
      transaction.objectStore('keys').put({key,iv,encrypted}, 'login');
      transaction.oncomplete = resolve;
      transaction.onerror = () => reject(Error('Synthetic database write failed'));
      transaction.onabort = () => reject(Error('Synthetic database write aborted'));
    });
  }
  const saved = await new Promise((resolve, reject) => {
    const request = database.transaction('keys').objectStore('keys').get('login');
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(Error('Synthetic database read failed'));
  });
  const session = document.cookie.split('; ').includes('syntheticSession=preserve');
  const persistent = document.cookie.split('; ').includes('syntheticPersistent=preserve');
  const local = localStorage.getItem('synthetic-session');
  window.storageState = {session, persistent, local:local === 'preserve', database:!!saved};
  if (isolated) {
    if (session || persistent || local !== null || saved !== undefined) throw Error('Cross-account data leak');
  } else {
    if (!session || !persistent || local !== 'preserve' || !saved) throw Error('Session did not persist');
    const decrypted = await crypto.subtle.decrypt({name:'AES-GCM',iv:saved.iv}, saved.key, saved.encrypted);
    if (new TextDecoder().decode(decrypted) !== 'synthetic login') throw Error('Stored key cannot decrypt');
  }
  database.close();
};
</script>"#;

#[tauri_runtime_cef::cef_entry_point]
fn main() {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        hook(panic);
        std::process::exit(1);
    }));
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_some_and(|value| value == "--phase") {
        phase(Path::new(&args[2]), args[3] == "write");
        return;
    }
    let root = tempfile::tempdir().unwrap();
    for name in ["home", "config", "data", "cache", "run", "tmp"] {
        storage::private_dir(&root.path().join(name)).unwrap();
    }
    for phase in ["write", "read", "read"] {
        eprintln!("STAGE: synthetic persistence {phase}");
        let ready = root.path().join("ready");
        if ready.exists() {
            std::fs::remove_file(&ready).unwrap();
        }
        let mut child = Command::new(std::env::current_exe().unwrap())
            .arg("--phase")
            .arg(root.path())
            .arg(phase)
            .env("HOME", root.path().join("home"))
            .env("XDG_CONFIG_HOME", root.path().join("config"))
            .env("XDG_DATA_HOME", root.path().join("data"))
            .env("XDG_CACHE_HOME", root.path().join("cache"))
            .env("XDG_RUNTIME_DIR", root.path().join("run"))
            .env("TMPDIR", root.path().join("tmp"))
            .process_group(0)
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while !ready.exists() {
            if let Some(status) = child.try_wait().unwrap() {
                panic!("Synthetic {phase} exited early: {status}");
            }
            if Instant::now() >= deadline {
                let _ = process_shutdown::stop(&mut child, Duration::ZERO);
                panic!("Synthetic persistence phase timed out");
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(process_shutdown::stop(&mut child, Duration::from_secs(10)).unwrap());
        assert!(child.wait().unwrap().success());
        if phase == "write" {
            // Exercise migration of a closed, fully populated synthetic profile.
            let chromium = root.path().join("profile/chromium");
            std::fs::create_dir_all(chromium.join("accounts")).unwrap();
            std::fs::rename(
                chromium.join(ACCOUNT),
                chromium.join("accounts").join(ACCOUNT),
            )
            .unwrap();
        }
    }
    println!("PASS: session and persistent cookies, localStorage, IndexedDB CryptoKey and account isolation survive two process restarts");
}

fn phase(root: &Path, write: bool) {
    let profile = root.join("profile");
    // This synthetic profile never contains real account data or system keyring keys.
    let runtime = account_browser::runtime(&profile)
        .unwrap()
        .secret_storage(SecretStorage::Mock);
    let ready = root.join("ready");
    tauri::Builder::default().runtime(runtime).setup(move |app| {
        let shutdown = Arc::new(AtomicBool::new(false));
        signal_hook::flag::register(libc::SIGTERM, shutdown.clone())?;
        let handle = app.handle().clone();
        gtk::glib::timeout_add_local(Duration::from_millis(50), move || {
            if shutdown.load(Ordering::Relaxed) {
                handle.exit(0);
                gtk::glib::ControlFlow::Break
            } else { gtk::glib::ControlFlow::Continue }
        });
        let window = tauri::window::WindowBuilder::new(app, "synthetic").inner_size(900.,640.).build()?;
        let mut views = Vec::new();
        for id in [ACCOUNT, OTHER] {
            let view = window.add_child(WebviewBuilder::new(id, WebviewUrl::External("about:blank".parse()?))
                .data_directory(account_profiles::account(&profile, id)?).devtools(false)
                .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
                .size(LogicalSize::new(800.,600.)))?;
            views.push(view);
        }
        std::thread::spawn(move || {
            for (index, view) in views.iter().enumerate() {
                fixture::serve(view, HTML);
                view.navigate("https://web.whatsapp.com/".parse().unwrap()).unwrap();
                fixture::wait_for(|| fixture::evaluate(view, "typeof window.checkSession") == "function");
                fixture::evaluate(view, &format!("window.checkSession({},{}).then(()=>window.result='passed').catch(()=>window.result='failed');true",write && index == 0,index != 0));
                fixture::wait_for(|| fixture::evaluate(view, "window.result || ''") != "");
                assert_eq!(fixture::evaluate(view, "window.result"), "passed", "Synthetic storage flags: {}", fixture::evaluate(view, "JSON.stringify(window.storageState || {})"));
            }
            assert_eq!(fixture::page_targets(&views[0]), 2, "Unexpected restored browser targets");
            std::fs::write(ready, b"synthetic").unwrap();
        });
        Ok(())
    }).run(tauri::generate_context!()).unwrap();
}

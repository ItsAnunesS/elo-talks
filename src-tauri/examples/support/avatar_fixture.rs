//! Prove native resource reuse works when cross-origin canvas access is denied.
use gtk::{glib, prelude::*};
use std::{
    cell::Cell,
    io::{Read, Write},
    net::TcpListener,
    rc::Rc,
    time::Duration,
};
use webkit2gtk::{WebContextExt, WebResourceExt, WebViewExt};

pub fn start() -> std::sync::Arc<std::sync::atomic::AtomicBool> {
    let pixel =
        gtk::gdk_pixbuf::Pixbuf::new(gtk::gdk_pixbuf::Colorspace::Rgb, true, 8, 16, 16).unwrap();
    pixel.fill(0xaabbccff);
    let bytes = pixel.save_to_bufferv("png", &[]).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = [0u8; 4096];
        let _ = stream.read(&mut request);
        write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", bytes.len()).unwrap();
        stream.write_all(&bytes).unwrap();
    });
    let complete = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let received = Rc::new(Cell::new(false));
    let context = webkit2gtk::WebContext::new_ephemeral();
    assert!(context.is_sandbox_enabled());
    let view = webkit2gtk::WebView::with_context(&context);
    let loaded = received.clone();
    let source = format!("http://{address}/avatar.png");
    let expected = source.clone();
    view.connect_resource_load_started(move |_, resource, _| {
        if resource.uri().as_deref() != Some(expected.as_str()) {
            return;
        }
        let loaded = loaded.clone();
        resource.connect_finished(move |resource| {
            let loaded = loaded.clone();
            WebResourceExt::data(resource, None::<&gtk::gio::Cancellable>, move |data| {
                let data = data.unwrap();
                assert!(crate::account_avatar::normalized_image(&data).is_some());
                loaded.set(true);
            });
        });
    });
    let html = format!(
        r#"<img id="avatar" src="{source}"><script>
const image = document.getElementById('avatar');
image.onload = () => {{
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = 64;
  canvas.getContext('2d').drawImage(image, 0, 0, 64, 64);
  try {{ canvas.toDataURL(); document.title = 'unexpected-access'; }}
  catch {{ document.title = 'canvas-blocked'; }}
}};
</script>"#
    );
    // localhost and 127.0.0.1 are deliberately different origins. No CORS grant.
    view.load_html(&html, Some("http://localhost/"));
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.add(&view);
    window.show_all();
    let result = complete.clone();
    let attempts = Cell::new(0);
    glib::timeout_add_local(Duration::from_millis(100), move || {
        attempts.set(attempts.get() + 1);
        assert!(attempts.get() < 100, "Native avatar fixture timed out");
        if received.get() && view.title().as_deref() == Some("canvas-blocked") {
            result.store(true, std::sync::atomic::Ordering::Relaxed);
            window.close();
            return glib::ControlFlow::Break;
        }
        glib::ControlFlow::Continue
    });
    complete
}

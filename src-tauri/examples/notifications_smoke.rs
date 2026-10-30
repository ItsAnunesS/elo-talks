//! Run only under dbus-run-session: no real desktop notifications or user data.
#[path = "../src/desktop_notifications.rs"]
mod desktop_notifications;
#[path = "../src/notification_content.rs"]
mod notification_content;
use gtk::{gio, glib, prelude::*};
use notification_content::{NotificationDetail, NotificationText};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

const SERVICE: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";
const XML: &str = r#"<node><interface name="org.freedesktop.Notifications"><method name="Notify">
<arg type="s" direction="in"/><arg type="u" direction="in"/><arg type="s" direction="in"/>
<arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="as" direction="in"/>
<arg type="a{sv}" direction="in"/><arg type="i" direction="in"/><arg type="u" direction="out"/>
</method><signal name="ActionInvoked"><arg type="u"/><arg type="s"/></signal>
<signal name="NotificationClosed"><arg type="u"/><arg type="u"/></signal></interface></node>"#;

async fn own_name(connection: &gio::DBusConnection, acquire: bool) {
    let parameters = if acquire {
        (SERVICE, 4u32).to_variant()
    } else {
        (SERVICE,).to_variant()
    };
    let reply = connection
        .call_future(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            if acquire {
                "RequestName"
            } else {
                "ReleaseName"
            },
            Some(&parameters),
            None,
            gio::DBusCallFlags::NONE,
            1000,
        )
        .await
        .unwrap();
    assert_eq!(
        reply.get::<(u32,)>(),
        Some((1,)),
        "Use an isolated D-Bus session"
    );
    glib::timeout_future(Duration::from_millis(30)).await;
}

fn main() {
    glib::MainContext::default().block_on(async {
        let connection = gio::bus_get_future(gio::BusType::Session).await.unwrap();
        // Refuse to claim an existing desktop service even if its owner permits replacement.
        own_name(&connection, true).await;
        let calls = Arc::new(Mutex::new(Vec::new()));
        let fail = Arc::new(AtomicBool::new(false));
        let records = calls.clone();
        let failed = fail.clone();
        let info = gio::DBusNodeInfo::for_xml(XML).unwrap();
        let registration = connection.register_object(PATH, &info.lookup_interface(SERVICE).unwrap(),
            move |_, _, _, _, _, parameters, invocation| {
                if failed.load(Ordering::Relaxed) {
                    invocation.return_dbus_error("org.freedesktop.Notifications.Error.Failed", "Synthetic private error");
                    return;
                }
                let (app, replaces, icon, title, body, actions, hints, timeout) = parameters.get::<(String, u32, String, String, String, Vec<String>, BTreeMap<String, glib::Variant>, i32)>().unwrap();
                assert_eq!(app, "Elo Talks");
                assert_eq!(icon, "dev.anuness.elotalks");
                assert_eq!(hints["desktop-entry"].get::<String>().unwrap(), "dev.anuness.elotalks");
                assert_eq!(timeout, 8000);
                let mut records = records.lock().unwrap();
                let id = if replaces == 0 { records.len() as u32 + 1 } else { replaces };
                records.push((replaces, id, actions, title, body));
                invocation.return_value(Some(&(id,).to_variant()));
            }, |_, _, _, _, _| ().to_variant(), |_, _, _, _, _, _| false).unwrap();
        let notifier = desktop_notifications::DesktopNotifications::default();
        let selected = Arc::new(Mutex::new(String::new()));
        for _ in 0..8 {
            let selected = selected.clone();
            assert_eq!(notifier.show("account-one", private_text(), move |id| *selected.lock().unwrap() = id.into()).await, Ok(true));
        }
        assert_eq!(calls.lock().unwrap().len(), 8, "Delivery must not stop after four undismissed notifications");
        assert!(calls.lock().unwrap().iter().skip(1).all(|call| call.0 == 1 && call.1 == 1));
        assert!(calls.lock().unwrap().iter().all(|call| call.3 == "Elo Talks" && call.4 == "You have a new message."));
        connection.emit_signal(None, PATH, SERVICE, "ActionInvoked", Some(&(1u32, "default").to_variant())).unwrap();
        glib::timeout_future(Duration::from_millis(30)).await;
        assert_eq!(*selected.lock().unwrap(), "account-one");
        assert_eq!(notifier.show("account-one", private_text(), |_| {}).await, Ok(true));
        assert_eq!(calls.lock().unwrap().last().unwrap().0, 0);
        assert_eq!(notifier.show("test", NotificationText::test(), |_| {}).await, Ok(true));
        assert!(calls.lock().unwrap().last().unwrap().2.is_empty());
        assert_eq!(calls.lock().unwrap().last().unwrap().4, "This is a test notification from Elo Talks.");
        let preview = NotificationText::message(NotificationDetail::Preview, "Portugal", Some("Maria & André"), Some("Hello <img src='https://example.invalid/'>"));
        assert_eq!(notifier.show("account-two", preview, |_| {}).await, Ok(true));
        {
            let records = calls.lock().unwrap();
            let preview = records.last().unwrap();
            assert_eq!(preview.3, "WhatsApp · Portugal");
            assert_eq!(preview.4, "Maria &amp; André\nHello &lt;img src=&apos;https://example.invalid/&apos;&gt;");
            assert_eq!(preview.2, ["default", "Open account"]);
        }
        let channel = NotificationText::message(NotificationDetail::Channel, "Work", Some("Private sender"), Some("Private message"));
        assert_eq!(notifier.show("account-two", channel, |_| {}).await, Ok(true));
        assert_eq!(calls.lock().unwrap().last().unwrap().3, "WhatsApp · Work");
        assert_eq!(calls.lock().unwrap().last().unwrap().4, "You have a new message.");
        fail.store(true, Ordering::Relaxed);
        let error = notifier.show("account-two", private_text(), |_| {}).await.unwrap_err();
        assert!(!error.contains("Synthetic private error"));
        fail.store(false, Ordering::Relaxed);
        assert_eq!(notifier.show("account-two", private_text(), |_| {}).await, Ok(true));
        own_name(&connection, false).await;
        assert!(notifier.show("account-one", private_text(), |_| {}).await.is_err());
        own_name(&connection, true).await;
        assert_eq!(notifier.show("account-one", private_text(), |_| {}).await, Ok(true));
        assert_eq!(calls.lock().unwrap().last().unwrap().0, 0, "Discard old IDs when the desktop service restarts");
        connection.unregister_object(registration).unwrap();
        println!("PASS: native D-Bus delivery, private/channel/escaped-preview payloads, grouping, account click, test notification, private errors and desktop-service restart");
    });
}

fn private_text() -> NotificationText {
    NotificationText::message(
        NotificationDetail::Private,
        "Private channel",
        Some("Private sender"),
        Some("Private message"),
    )
}

//! Asynchronous Linux notifications. One connection and one retained popup per account.
use crate::notification_content::NotificationText;
use gtk::{gio, prelude::*};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex, OnceLock},
};

const SERVICE: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";
const ERROR: &str =
    "Linux could not accept the notification. Check your desktop notification service.";

#[derive(Default)]
pub struct DesktopNotifications {
    connection: OnceLock<gio::DBusConnection>,
    targets: Arc<Mutex<BTreeMap<String, u32>>>,
    pending: Mutex<BTreeSet<String>>,
}

struct Pending<'a> {
    notifier: &'a DesktopNotifications,
    key: String,
}
impl Drop for Pending<'_> {
    fn drop(&mut self) {
        self.notifier.pending.lock().unwrap().remove(&self.key);
    }
}

impl DesktopNotifications {
    pub async fn show(
        &self,
        key: &str,
        text: NotificationText,
        on_open: impl Fn(&str) + 'static,
    ) -> Result<bool, &'static str> {
        {
            let mut pending = self.pending.lock().unwrap();
            if pending.contains(key) {
                // Coalesce a burst while the desktop is accepting this account's popup.
                return Ok(false);
            }
            if pending.len() >= 21 {
                return Err("Too many notification requests are pending. Try again shortly.");
            }
            pending.insert(key.to_owned());
        }
        let _pending = Pending {
            notifier: self,
            key: key.to_owned(),
        };
        if self.connection.get().is_none() {
            let connection = gio::bus_get_future(gio::BusType::Session)
                .await
                .map_err(|_| ERROR)?;
            if self.connection.set(connection.clone()).is_ok() {
                let targets = self.targets.clone();
                connection.signal_subscribe(
                    Some("org.freedesktop.DBus"),
                    Some("org.freedesktop.DBus"),
                    Some("NameOwnerChanged"),
                    Some("/org/freedesktop/DBus"),
                    Some(SERVICE),
                    gio::DBusSignalFlags::NONE,
                    move |_, _, _, _, _, _| {
                        targets.lock().unwrap().clear();
                    },
                );
                let targets = self.targets.clone();
                connection.signal_subscribe(
                    Some(SERVICE),
                    Some(SERVICE),
                    None,
                    Some(PATH),
                    None,
                    gio::DBusSignalFlags::NONE,
                    move |_, _, _, _, member, parameters| {
                        let (id, open) = match member {
                            "ActionInvoked" => match parameters.get::<(u32, String)>() {
                                Some((id, action)) if action == "default" => (id, true),
                                _ => return,
                            },
                            "NotificationClosed" => match parameters.get::<(u32, u32)>() {
                                Some((id, _)) => (id, false),
                                _ => return,
                            },
                            _ => return,
                        };
                        let key = {
                            let mut targets = targets.lock().unwrap();
                            let key = targets
                                .iter()
                                .find(|(_, value)| **value == id)
                                .map(|(key, _)| key.clone());
                            if let Some(key) = &key {
                                targets.remove(key);
                            }
                            key
                        };
                        if open {
                            if let Some(key) = key {
                                on_open(&key);
                            }
                        }
                    },
                );
            }
        }
        let replaces = self.targets.lock().unwrap().get(key).copied().unwrap_or(0);
        let hints = BTreeMap::from([
            ("desktop-entry", "dev.anuness.elotalks".to_variant()),
            ("category", "im.received".to_variant()),
        ]);
        let actions = if text.actionable {
            vec!["default", "Open account"]
        } else {
            vec![]
        };
        let parameters = (
            "Elo Talks",
            replaces,
            "dev.anuness.elotalks",
            text.summary,
            text.body,
            actions,
            hints,
            8000i32,
        )
            .to_variant();
        let result = self
            .connection
            .get()
            .unwrap()
            .call_future(
                Some(SERVICE),
                PATH,
                SERVICE,
                "Notify",
                Some(&parameters),
                None,
                gio::DBusCallFlags::NONE,
                3000,
            )
            .await
            .map_err(|_| ERROR)?;
        let (id,) = result
            .get::<(u32,)>()
            .filter(|(id,)| *id != 0)
            .ok_or(ERROR)?;
        let mut targets = self.targets.lock().unwrap();
        // Bound metadata even when accounts are repeatedly removed and recreated.
        if !targets.contains_key(key) && targets.len() >= 21 {
            targets.pop_first();
        }
        targets.insert(key.to_owned(), id);
        Ok(true)
    }
}

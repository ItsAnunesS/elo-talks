//! Privacy-preserving account health and notification state. No message content is retained.
use crate::notification_content::NotificationDetail;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationPreferences {
    #[serde(default)]
    pub detail: NotificationDetail,
    #[serde(default)]
    pub do_not_disturb: bool,
    #[serde(default)]
    pub muted_accounts: BTreeSet<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PageState {
    #[default]
    Loading,
    Ready,
    LoadFailed,
    TlsFailed,
    ProcessStopped,
    SandboxUnavailable,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NotificationDelivery {
    #[default]
    None,
    Muted,
    DoNotDisturb,
    Pending,
    Submitted,
    Failed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Readiness {
    #[default]
    Preparing,
    Chats,
    QrCode,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AvatarState {
    #[default]
    Waiting,
    Searching,
    ControlNotFound,
    ResourceUnavailable,
    UnsupportedImage,
    DecodeFailed,
    ReadFailed,
    Ready,
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountHealth {
    pub page: PageState,
    pub readiness: Readiness,
    pub avatar: AvatarState,
    pub avatar_probe_active: bool,
    pub avatar_attempts: u8,
    pub pending_notifications: u32,
    pub unread_count: Option<u32>,
    pub native_notifications: u64,
    pub notification_delivery: NotificationDelivery,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountRuntime {
    pub accounts: BTreeMap<String, AccountHealth>,
    pub preferences: NotificationPreferences,
    #[serde(skip)]
    pub selected: Option<String>,
    #[serde(skip)]
    pub focused: bool,
    #[serde(skip)]
    pub channel_names: BTreeMap<String, String>,
}

impl AccountHealth {
    pub fn badge_count(&self) -> u32 {
        self.unread_count.unwrap_or(self.pending_notifications)
    }
}

impl AccountRuntime {
    pub fn update_unread(&mut self, id: &str, count: Option<u32>) -> bool {
        let Some(account) = self.accounts.get_mut(id) else {
            return false;
        };
        if account.unread_count == count {
            return false;
        }
        account.unread_count = count;
        true
    }

    pub fn update_page(&mut self, id: &str, page: PageState) {
        if let Some(account) = self.accounts.get_mut(id) {
            // WebKit emits Finished after failure too; never hide the failure with Ready.
            if page != PageState::Ready || account.page == PageState::Loading {
                account.page = page;
                if page != PageState::Ready {
                    account.unread_count = None;
                    account.readiness = Readiness::Preparing;
                }
            }
        }
    }

    pub fn acknowledge_selected(&mut self) {
        if self.focused {
            if let Some(account) = self
                .selected
                .as_ref()
                .and_then(|id| self.accounts.get_mut(id))
            {
                account.pending_notifications = 0;
            }
        }
    }

    pub fn notification(&mut self, id: &str) -> bool {
        let Some(account) = self.accounts.get_mut(id) else {
            return false;
        };
        account.native_notifications = account.native_notifications.saturating_add(1);
        // WhatsApp decides whether a conversation warrants a notification. A selected
        // account may contain other chats; window focus must not discard their events.
        account.pending_notifications = account.pending_notifications.saturating_add(1);
        account.notification_delivery = if self.preferences.do_not_disturb {
            NotificationDelivery::DoNotDisturb
        } else if self.preferences.muted_accounts.contains(id) {
            NotificationDelivery::Muted
        } else {
            NotificationDelivery::Pending
        };
        account.notification_delivery == NotificationDelivery::Pending
    }

    pub fn diagnostics(&self) -> serde_json::Value {
        serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"), "platform": "linux", "architecture": std::env::consts::ARCH,
            "accounts": self.accounts.values().collect::<Vec<_>>(),
            "doNotDisturb": self.preferences.do_not_disturb,
            "badgeProtocol": "Unity LauncherEntry (requires desktop support)",
            "counter": "WhatsApp page-title unread counter when available; otherwise native notifications since last viewed. Not a guaranteed message total.",
            "pageState": "Page load plus bounded structural startup readiness; not messaging connection health"
        })
    }

    pub fn total(&self) -> u64 {
        self.accounts
            .values()
            .map(|account| u64::from(account.badge_count()))
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn runtime() -> AccountRuntime {
        let mut state = AccountRuntime::default();
        state
            .accounts
            .insert("one".into(), AccountHealth::default());
        state
            .accounts
            .insert("two".into(), AccountHealth::default());
        state.selected = Some("one".into());
        state
    }
    #[test]
    fn counts_selected_account_on_another_workspace() {
        let mut state = runtime();
        assert!(state.notification("one"));
        state.focused = true;
        state.acknowledge_selected();
        assert_eq!(state.total(), 0);
        assert!(state.notification("one"));
        assert!(state.notification("two"));
        assert_eq!(state.total(), 2);
    }
    #[test]
    fn muted_and_dnd_accounts_keep_badges_without_popups() {
        let mut state = runtime();
        state.preferences.muted_accounts.insert("one".into());
        assert!(!state.notification("one"));
        state.preferences.do_not_disturb = true;
        assert!(!state.notification("two"));
        assert_eq!(state.total(), 2);
        assert!(!state.notification("unknown"));
    }
    #[test]
    fn notification_diagnostics_distinguish_receipt_from_suppression() {
        let mut state = runtime();
        state.focused = true;
        assert!(state.notification("one"));
        assert_eq!(state.accounts["one"].native_notifications, 1);
        assert_eq!(state.accounts["one"].pending_notifications, 1);
        assert_eq!(
            state.accounts["one"].notification_delivery,
            NotificationDelivery::Pending
        );
        state.focused = false;
        state.preferences.muted_accounts.insert("one".into());
        assert!(!state.notification("one"));
        assert_eq!(
            state.accounts["one"].notification_delivery,
            NotificationDelivery::Muted
        );
        state.preferences.do_not_disturb = true;
        assert!(!state.notification("one"));
        assert_eq!(
            state.accounts["one"].notification_delivery,
            NotificationDelivery::DoNotDisturb
        );
        state.preferences = NotificationPreferences::default();
        assert!(state.notification("one"));
        assert_eq!(state.accounts["one"].native_notifications, 4);
        assert_eq!(state.accounts["one"].pending_notifications, 4);
        assert_eq!(
            state.accounts["one"].notification_delivery,
            NotificationDelivery::Pending
        );
    }
    #[test]
    fn finish_after_failure_does_not_report_success() {
        let mut state = runtime();
        state.update_page("one", PageState::TlsFailed);
        state.update_page("one", PageState::Ready);
        assert_eq!(state.accounts["one"].page, PageState::TlsFailed);
        state.update_page("one", PageState::Loading);
        state.update_page("one", PageState::Ready);
        assert_eq!(state.accounts["one"].page, PageState::Ready);
    }
    #[test]
    fn diagnostics_exclude_account_identifiers_and_preferences() {
        let mut state = runtime();
        state
            .channel_names
            .insert("one".into(), "Private channel name".into());
        state.accounts.insert(
            "private-account-identifier".into(),
            AccountHealth::default(),
        );
        state
            .preferences
            .muted_accounts
            .insert("private-account-identifier".into());
        let report = state.diagnostics().to_string();
        assert!(!report.contains("private-account-identifier"));
        assert!(!report.contains("mutedAccounts"));
        assert!(!report.contains("Private channel name"));
        let snapshot = serde_json::to_string(&state).unwrap();
        assert!(!snapshot.contains("Private channel name"));
        assert!(report.contains("version"));
    }

    #[test]
    fn unread_badges_follow_whatsapp_and_do_not_clear_on_selection() {
        let mut state = runtime();
        assert!(state.update_unread("one", Some(12)));
        assert!(!state.update_unread("one", Some(12)));
        state.focused = true;
        state.notification("one");
        state.acknowledge_selected();
        assert_eq!(state.total(), 12);
        state.update_unread("one", Some(4));
        assert_eq!(state.total(), 4);
        state.update_unread("one", Some(0));
        state.notification("one");
        assert_eq!(state.total(), 0);
        state.update_page("one", PageState::ProcessStopped);
        assert_eq!(state.accounts["one"].unread_count, None);
        assert_eq!(state.total(), 1);
        assert!(!state.update_unread("unknown", Some(50)));
    }

    #[test]
    fn existing_preferences_do_not_enable_message_previews() {
        let existing: NotificationPreferences =
            serde_json::from_str(r#"{"doNotDisturb":false,"mutedAccounts":[]}"#).unwrap();
        assert_eq!(existing.detail, NotificationDetail::Private);
        assert!(
            serde_json::from_str::<NotificationPreferences>(r#"{"detail":"unknown"}"#).is_err()
        );
    }
}

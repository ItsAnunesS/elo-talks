//! Privacy-preserving account health and notification state. No message content is retained.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationPreferences {
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

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountHealth {
    pub page: PageState,
    pub pending_notifications: u32,
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
}

impl AccountRuntime {
    pub fn update_page(&mut self, id: &str, page: PageState) {
        if let Some(account) = self.accounts.get_mut(id) {
            // WebKit emits Finished after failure too; never hide the failure with Ready.
            if page != PageState::Ready || account.page == PageState::Loading {
                account.page = page;
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
        if self.focused && self.selected.as_deref() == Some(id) {
            return false;
        }
        account.pending_notifications = account.pending_notifications.saturating_add(1);
        !self.preferences.do_not_disturb && !self.preferences.muted_accounts.contains(id)
    }

    pub fn diagnostics(&self) -> serde_json::Value {
        serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"), "platform": "linux", "architecture": std::env::consts::ARCH,
            "accounts": self.accounts.values().collect::<Vec<_>>(),
            "doNotDisturb": self.preferences.do_not_disturb,
            "badgeProtocol": "Unity LauncherEntry (requires desktop support)",
            "counter": "Native notifications since account last viewed; not WhatsApp unread totals",
            "pageState": "Page load only; QR and WhatsApp connection state are not inspected"
        })
    }

    pub fn total(&self) -> u64 {
        self.accounts
            .values()
            .map(|account| u64::from(account.pending_notifications))
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
        assert!(!state.notification("one"));
        assert!(state.notification("two"));
        assert_eq!(state.total(), 1);
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
        assert!(report.contains("version"));
    }
}

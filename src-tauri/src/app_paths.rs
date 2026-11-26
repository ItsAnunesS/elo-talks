//! Separate development data and IPC from installed accounts.
pub fn namespace(development: bool) -> &'static str {
    if development {
        "elo-talks-dev"
    } else {
        "elo-talks"
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn development_cannot_select_installed_data_or_ipc() {
        let root = std::path::Path::new("/synthetic");
        let production = root.join(super::namespace(false));
        let development = root.join(super::namespace(true));
        assert_eq!(production, root.join("elo-talks"));
        assert_ne!(development, production);
        assert_ne!(
            development.join("control.sock"),
            production.join("control.sock")
        );
    }
}

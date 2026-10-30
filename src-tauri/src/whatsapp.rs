//! Narrow adapter for public WhatsApp page metadata; never reads conversations.

pub fn is_origin(url: &tauri::Url) -> bool {
    url.scheme() == "https"
        && url.host_str() == Some("web.whatsapp.com")
        && url.port_or_known_default() == Some(443)
        && url.username().is_empty()
        && url.password().is_none()
}

pub fn unread_count(uri: Option<&str>, title: Option<&str>) -> Option<u32> {
    if !is_origin(&tauri::Url::parse(uri?).ok()?) {
        return None;
    }
    let title = title?;
    if title == "WhatsApp" {
        return Some(0);
    }
    let count = title.strip_prefix('(')?.strip_suffix(") WhatsApp")?;
    if count.is_empty() || count.len() > 6 || !count.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    count.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_only_bounded_counts_from_the_exact_origin() {
        let origin = Some("https://web.whatsapp.com/");
        assert_eq!(unread_count(origin, Some("WhatsApp")), Some(0));
        assert_eq!(unread_count(origin, Some("(19) WhatsApp")), Some(19));
        for title in [
            "(12) Private chat",
            "(9999999) WhatsApp",
            "(-1) WhatsApp",
            "(1.2) WhatsApp",
            "(1) WhatsApp\n",
            "(9+) WhatsApp",
            "(１２) WhatsApp",
        ] {
            assert_eq!(unread_count(origin, Some(title)), None);
        }
        for uri in [
            "http://web.whatsapp.com/",
            "https://web.whatsapp.com.evil.test/",
            "https://web.whatsapp.com:444/",
            "https://user@web.whatsapp.com/",
            "about:blank",
        ] {
            assert_eq!(unread_count(Some(uri), Some("(9) WhatsApp")), None);
        }
    }
}

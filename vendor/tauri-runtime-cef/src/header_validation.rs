// Copyright 2026 Elo Talks contributors
// SPDX-License-Identifier: MIT

use http::{header::HeaderName, HeaderMap, HeaderValue};

pub(crate) fn append(headers: &mut HeaderMap, name: &str, value: &str) -> Option<()> {
    let name = HeaderName::from_bytes(name.as_bytes()).ok()?;
    let value = HeaderValue::from_str(value).ok()?;
    headers.append(name, value);
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_browser_headers_are_rejected_without_panicking_or_mutating() {
        let mut headers = HeaderMap::new();
        for (name, value) in [("bad name", "value"), (":authority", "host"), ("valid", "line\r\nbreak"), ("", "value")] {
            assert!(append(&mut headers, name, value).is_none());
            assert!(headers.is_empty());
        }
    }

    #[test]
    fn valid_headers_preserve_origin_and_repeated_values() {
        let mut headers = HeaderMap::new();
        append(&mut headers, "Origin", "https://web.whatsapp.com").unwrap();
        append(&mut headers, "x-example", "one").unwrap();
        append(&mut headers, "x-example", "two").unwrap();
        assert_eq!(headers["origin"], "https://web.whatsapp.com");
        assert_eq!(headers.get_all("x-example").iter().count(), 2);
    }
}

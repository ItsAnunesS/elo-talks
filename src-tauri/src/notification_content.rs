//! Bounded desktop text built from native notifications, never conversation DOM.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NotificationDetail {
    #[default]
    Private,
    Channel,
    Preview,
}

pub struct NotificationText {
    pub summary: String,
    pub body: String,
    pub actionable: bool,
}

impl NotificationText {
    pub fn message(
        detail: NotificationDetail,
        channel: &str,
        sender: Option<&str>,
        message: Option<&str>,
    ) -> Self {
        let mut text = Self {
            summary: "Elo Talks".into(),
            body: "You have a new message.".into(),
            actionable: true,
        };
        if detail == NotificationDetail::Private {
            return text;
        }
        let channel = plain_text(channel, 80);
        text.summary = if channel.is_empty() {
            "WhatsApp".into()
        } else {
            format!("WhatsApp · {channel}")
        };
        if detail == NotificationDetail::Preview {
            let sender = plain_text(sender.unwrap_or_default(), 100);
            let message = plain_text(message.unwrap_or_default(), 300);
            let message = if message.is_empty() {
                "You have a new message."
            } else {
                &message
            };
            let preview = if sender.is_empty() {
                message.to_owned()
            } else {
                format!("{sender}\n{message}")
            };
            // Notification bodies may support markup. Render WhatsApp text literally,
            // including apparent links/images/tags, without loading external resources.
            text.body = gtk::glib::markup_escape_text(&preview).into();
        }
        text
    }

    pub fn test() -> Self {
        Self {
            summary: "Elo Talks".into(),
            body: "This is a test notification from Elo Talks.".into(),
            actionable: false,
        }
    }
}

fn plain_text(value: &str, limit: usize) -> String {
    let mut text = String::new();
    for (index, character) in value.chars().take(limit + 1).enumerate() {
        if index == limit {
            text.push('…');
            break;
        }
        if character.is_whitespace() {
            if !text.is_empty() && !text.ends_with(' ') {
                text.push(' ');
            }
        } else if !character.is_control()
            && !matches!(character, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        {
            text.push(character);
        }
    }
    text.trim_end().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn privacy_levels_never_include_unrequested_details() {
        let private = NotificationText::message(
            NotificationDetail::Private,
            "Private channel",
            Some("Private sender"),
            Some("Private message"),
        );
        assert_eq!(private.summary, "Elo Talks");
        assert_eq!(private.body, "You have a new message.");
        let channel = NotificationText::message(
            NotificationDetail::Channel,
            "Portugal",
            Some("Private sender"),
            Some("Private message"),
        );
        assert_eq!(channel.summary, "WhatsApp · Portugal");
        assert_eq!(channel.body, private.body);
    }

    #[test]
    fn preview_escapes_remote_markup_and_preserves_unicode() {
        let preview = NotificationText::message(
            NotificationDetail::Preview,
            "Portugal\n\u{202e}",
            Some("Maria & André"),
            Some("\0Olá 👩‍💻 <img src='https://example.invalid/photo'>\nAté já"),
        );
        assert_eq!(preview.summary, "WhatsApp · Portugal");
        assert!(preview
            .body
            .starts_with("Maria &amp; André\nOlá 👩‍💻 &lt;img"));
        assert!(!preview.body.contains('<') && !preview.body.contains('>'));
        assert!(!preview.body.contains('\0'));
        assert!(preview.body.ends_with("Até já"));
    }

    #[test]
    fn preview_falls_back_when_whatsapp_omits_content_and_bounds_payloads() {
        let missing = NotificationText::message(NotificationDetail::Preview, "", None, None);
        assert_eq!(missing.summary, "WhatsApp");
        assert_eq!(missing.body, "You have a new message.");
        let large = "😃".repeat(10_000);
        let preview = NotificationText::message(
            NotificationDetail::Preview,
            &large,
            Some(&large),
            Some(&large),
        );
        assert!(preview.summary.chars().count() < 100);
        assert_eq!(preview.body.chars().count(), 403);
        assert!(preview.body.ends_with('…'));
    }
}

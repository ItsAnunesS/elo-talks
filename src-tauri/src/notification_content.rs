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

pub struct NotificationImage {
    rowstride: i32,
    alpha: bool,
    channels: i32,
    pixels: Vec<u8>,
}
impl NotificationImage {
    pub fn from_data_url(value: &str) -> Option<Self> {
        use gtk::prelude::*;
        if value.len() > 44_000 {
            return None;
        }
        let raw = gtk::glib::base64_decode(value.strip_prefix("data:image/png;base64,")?);
        if raw.get(..8)? != b"\x89PNG\r\n\x1a\n"
            || raw.get(12..16)? != b"IHDR"
            || u32::from_be_bytes(raw.get(16..20)?.try_into().ok()?) != 64
            || u32::from_be_bytes(raw.get(20..24)?.try_into().ok()?) != 64
        {
            return None;
        }
        let loader = gtk::gdk_pixbuf::PixbufLoader::with_type("png").ok()?;
        loader.write(&raw).ok()?;
        loader.close().ok()?;
        let image = loader.pixbuf()?;
        let channels = image.n_channels();
        let rowstride = image.rowstride();
        if image.width() != 64
            || image.height() != 64
            || image.bits_per_sample() != 8
            || !matches!(channels, 3 | 4)
            || rowstride != 64 * channels
        {
            return None;
        }
        let pixels = image.read_pixel_bytes().as_ref().to_vec();
        if pixels.len() != (rowstride * 64) as usize {
            return None;
        }
        Some(Self {
            rowstride,
            alpha: image.has_alpha(),
            channels,
            pixels,
        })
    }
    pub fn variant(&self) -> gtk::glib::Variant {
        use gtk::glib::variant::ToVariant;
        (
            64i32,
            64i32,
            self.rowstride,
            self.alpha,
            8i32,
            self.channels,
            self.pixels.as_slice(),
        )
            .to_variant()
    }
}

pub struct NotificationText {
    pub image: Option<NotificationImage>,
    pub sequence: Option<u64>,
    detail: NotificationDetail,
    pub summary: String,
    pub body: String,
    pub actionable: bool,
}

impl NotificationText {
    pub fn message(
        detail: NotificationDetail,
        workspace: &str,
        channel: &str,
        sender: Option<&str>,
        message: Option<&str>,
    ) -> Self {
        let mut text = Self {
            image: None,
            sequence: None,
            detail,
            summary: "Elo Talks".into(),
            body: "You have a new message.".into(),
            actionable: true,
        };
        if detail == NotificationDetail::Private {
            return text;
        }
        let channel = plain_text(channel, 80);
        let workspace = plain_text(workspace, 80);
        let channel = match (workspace.is_empty(), channel.is_empty()) {
            (false, false) => format!("{workspace} / {channel}"),
            (false, true) => workspace,
            _ => channel,
        };
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

    pub fn restrict(
        mut self,
        detail: NotificationDetail,
        workspace: &str,
        channel: &str,
        photo: bool,
    ) -> Self {
        if detail == NotificationDetail::Private || self.detail == NotificationDetail::Private {
            let mut redacted = Self::message(NotificationDetail::Private, "", "", None, None);
            redacted.sequence = self.sequence;
            return redacted;
        }
        if detail == NotificationDetail::Channel || self.detail == NotificationDetail::Channel {
            let mut redacted =
                Self::message(NotificationDetail::Channel, workspace, channel, None, None);
            redacted.sequence = self.sequence;
            return redacted;
        }
        if !photo {
            self.image = None;
        }
        self
    }

    pub fn test() -> Self {
        Self {
            image: None,
            sequence: None,
            detail: NotificationDetail::Private,
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
    fn images_are_bounded_and_removed_when_privacy_changes() {
        let pixbuf =
            gtk::gdk_pixbuf::Pixbuf::new(gtk::gdk_pixbuf::Colorspace::Rgb, true, 8, 64, 64)
                .unwrap();
        pixbuf.fill(0x663399ff);
        let raw = pixbuf.save_to_bufferv("png", &[]).unwrap();
        let data = format!("data:image/png;base64,{}", gtk::glib::base64_encode(&raw));
        let image = NotificationImage::from_data_url(&data).unwrap();
        assert_eq!(image.variant().type_().as_str(), "(iiibiiay)");
        assert!(NotificationImage::from_data_url("data:image/png;base64,broken").is_none());
        let mut oversized = raw.clone();
        oversized[16..20].copy_from_slice(&4096u32.to_be_bytes());
        assert!(NotificationImage::from_data_url(&format!(
            "data:image/png;base64,{}",
            gtk::glib::base64_encode(&oversized)
        ))
        .is_none());
        let mut preview = NotificationText::message(
            NotificationDetail::Preview,
            "Personal",
            "Portugal",
            Some("Sender"),
            Some("Secret"),
        );
        preview.image = Some(image);
        preview.sequence = Some(9);
        let redacted = preview.restrict(NotificationDetail::Channel, "Personal", "Portugal", true);
        assert_eq!(redacted.summary, "WhatsApp · Personal / Portugal");
        assert_eq!(redacted.body, "You have a new message.");
        assert!(redacted.image.is_none());
        assert_eq!(redacted.sequence, Some(9));
        let private = redacted.restrict(NotificationDetail::Private, "Personal", "Portugal", true);
        assert_eq!(private.summary, "Elo Talks");
        let private = private.restrict(NotificationDetail::Preview, "Personal", "Portugal", true);
        assert_eq!(private.summary, "Elo Talks");
        assert_eq!(private.body, "You have a new message.");
    }

    #[test]
    fn privacy_levels_never_include_unrequested_details() {
        let private = NotificationText::message(
            NotificationDetail::Private,
            "",
            "Private channel",
            Some("Private sender"),
            Some("Private message"),
        );
        assert_eq!(private.summary, "Elo Talks");
        assert_eq!(private.body, "You have a new message.");
        let channel = NotificationText::message(
            NotificationDetail::Channel,
            "",
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
            "",
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
        let missing = NotificationText::message(NotificationDetail::Preview, "", "", None, None);
        assert_eq!(missing.summary, "WhatsApp");
        assert_eq!(missing.body, "You have a new message.");
        let large = "😃".repeat(10_000);
        let preview = NotificationText::message(
            NotificationDetail::Preview,
            "",
            &large,
            Some(&large),
            Some(&large),
        );
        assert!(preview.summary.chars().count() < 100);
        assert_eq!(preview.body.chars().count(), 403);
        assert!(preview.body.ends_with('…'));
    }
}

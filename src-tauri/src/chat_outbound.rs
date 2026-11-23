//! Outbound attachments for native chat replies.
//!
//! The runtime persists tool images (screenshots) and other attachments on the
//! assistant message. This module turns those attachments into bytes that a
//! provider sender can upload, with the same size discipline as inbound files.

use base64::Engine;
use ravenbot_core::{Attachment, MessageContent, MessageRole};
use ravenbot_db::queries::MessageQueries;
use ravenbot_db::Database;
use uuid::Uuid;

pub const MAX_OUTBOUND_BYTES: u64 = 20 * 1024 * 1024;
pub const MAX_OUTBOUND_IMAGE_BYTES: u64 = 10 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct OutboundFile {
    pub name: String,
    pub mime_type: String,
    pub bytes: Vec<u8>,
    pub is_image: bool,
}

impl OutboundFile {
    pub fn from_attachment(attachment: &Attachment) -> Result<Self, String> {
        let bytes = if let Some(data) = attachment.data.as_deref() {
            base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(|_| "The image attachment is not valid base64.".to_string())?
        } else {
            std::fs::read(&attachment.path)
                .map_err(|error| format!("The attachment could not be read: {error}"))?
        };
        if bytes.len() as u64 > MAX_OUTBOUND_BYTES {
            return Err("The attachment is larger than 20 MB.".to_string());
        }
        let is_image = attachment.is_image || attachment.mime_type.starts_with("image/");
        if is_image && bytes.len() as u64 > MAX_OUTBOUND_IMAGE_BYTES {
            return Err("The image is larger than 10 MB.".to_string());
        }
        Ok(Self {
            name: if attachment.name.trim().is_empty() {
                "attachment".to_string()
            } else {
                attachment.name.clone()
            },
            mime_type: if attachment.mime_type.trim().is_empty() {
                "application/octet-stream".to_string()
            } else {
                attachment.mime_type.clone()
            },
            bytes,
            is_image,
        })
    }
}

/// The last assistant text plus any attachments persisted with that message.
pub async fn last_assistant_reply(
    db: &Database,
    thread_id: Uuid,
) -> Option<(String, Vec<Attachment>)> {
    let messages = MessageQueries::list_by_thread(db.pool(), thread_id)
        .await
        .ok()?;
    let message = messages
        .into_iter()
        .rev()
        .find(|message| message.role == MessageRole::Assistant)?;
    let text = match &message.content {
        MessageContent::Text { text, .. } => text.clone(),
        MessageContent::Checklist { text, items } => {
            let items = items
                .iter()
                .map(|item| {
                    format!(
                        "{} {}",
                        match item.status {
                            ravenbot_core::ChecklistStatus::Completed => "✓",
                            ravenbot_core::ChecklistStatus::Failed => "✗",
                            _ => "○",
                        },
                        item.label
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            text.clone()
                .filter(|value| !value.trim().is_empty())
                .map(|value| format!("{value}\n{items}"))
                .unwrap_or(items)
        }
        _ => String::new(),
    };
    let attachments = message
        .attachments
        .into_iter()
        .filter(|attachment| OutboundFile::from_attachment(attachment).is_ok())
        .collect::<Vec<_>>();
    Some((text, attachments))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_and_disk_attachments_are_read() {
        let inline = Attachment {
            id: Uuid::new_v4(),
            name: "shot.png".to_string(),
            mime_type: "image/png".to_string(),
            size: 4,
            path: "/nonexistent".to_string(),
            data: Some(base64::engine::general_purpose::STANDARD.encode(b"png!")),
            is_image: true,
        };
        let file = OutboundFile::from_attachment(&inline).expect("inline");
        assert_eq!(file.name, "shot.png");
        assert_eq!(file.bytes, b"png!");

        let path = std::env::temp_dir().join(format!("ravenbot-out-{}", Uuid::new_v4()));
        std::fs::write(&path, b"data").expect("write");
        let disk = Attachment {
            id: Uuid::new_v4(),
            name: "notes.txt".to_string(),
            mime_type: "text/plain".to_string(),
            size: 4,
            path: path.to_string_lossy().to_string(),
            data: None,
            is_image: false,
        };
        let file = OutboundFile::from_attachment(&disk).expect("disk");
        assert_eq!(file.bytes, b"data");
        let _ = std::fs::remove_file(path);
    }
}

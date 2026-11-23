//! Shared download/cap logic for inbound chat attachments.
//!
//! Files are written under the account cache, never executed, and only images
//! are passed to the model inline. Non-image files are still recorded on the
//! message so the user can open them from the workspace. A chat attachment is
//! capped at 20 MB; images are additionally capped at 10 MB for inline base64.

use std::path::Path;

use base64::Engine;
use ravenbot_core::Attachment;
use uuid::Uuid;

pub const MAX_ATTACHMENT_BYTES: u64 = 20 * 1024 * 1024;
pub const MAX_IMAGE_BYTES: u64 = 10 * 1024 * 1024;
pub const MAX_ATTACHMENTS: usize = 4;

fn safe_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_' | ' '))
        .take(96)
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        "attachment".to_string()
    } else {
        cleaned.to_string()
    }
}

fn attachment_dir() -> std::path::PathBuf {
    ravenbot_core::cache_dir().join("chat-attachments")
}

#[allow(clippy::too_many_arguments)]
pub async fn download(
    client: &reqwest::Client,
    url: &str,
    bearer: Option<&str>,
    name: &str,
    mime_hint: Option<&str>,
    size_hint: Option<u64>,
) -> Result<Attachment, String> {
    if size_hint.is_some_and(|size| size > MAX_ATTACHMENT_BYTES) {
        return Err("The attachment is larger than 20 MB.".to_string());
    }
    let mut request = client.get(url);
    if let Some(token) = bearer {
        request = request.bearer_auth(token);
    }
    let response = request
        .send()
        .await
        .map_err(|_| "The attachment download failed.".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "The attachment download returned HTTP {}.",
            response.status().as_u16()
        ));
    }
    if response
        .content_length()
        .is_some_and(|size| size > MAX_ATTACHMENT_BYTES)
    {
        return Err("The attachment is larger than 20 MB.".to_string());
    }
    let mime_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(';').next().unwrap_or(value).trim().to_string())
        .or_else(|| mime_hint.map(|value| value.to_string()))
        .unwrap_or_else(|| "application/octet-stream".to_string());
    let bytes = response
        .bytes()
        .await
        .map_err(|_| "The attachment body could not be read.".to_string())?;
    if bytes.len() as u64 > MAX_ATTACHMENT_BYTES {
        return Err("The attachment is larger than 20 MB.".to_string());
    }
    let is_image = mime_type.starts_with("image/");
    if is_image && bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Err("The image is larger than 10 MB.".to_string());
    }
    let name = safe_name(name);
    let dir = attachment_dir();
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|error| error.to_string())?;
    let path = dir.join(format!("{}-{name}", Uuid::new_v4()));
    tokio::fs::write(&path, &bytes)
        .await
        .map_err(|error| error.to_string())?;
    let data = if is_image {
        Some(base64::engine::general_purpose::STANDARD.encode(&bytes))
    } else {
        None
    };
    Ok(Attachment {
        id: Uuid::new_v4(),
        name,
        mime_type,
        size: bytes.len() as u64,
        path: path.to_string_lossy().to_string(),
        data,
        is_image,
    })
}

/// Decode a base64 payload supplied directly by a provider (Teams inline
/// attachments) without writing it separately.
pub fn from_base64(name: &str, mime_type: &str, data: &str) -> Result<Attachment, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|_| "The inline attachment is not valid base64.".to_string())?;
    if bytes.len() as u64 > MAX_ATTACHMENT_BYTES {
        return Err("The attachment is larger than 20 MB.".to_string());
    }
    let is_image = mime_type.starts_with("image/");
    if is_image && bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Err("The image is larger than 10 MB.".to_string());
    }
    let name = safe_name(name);
    let dir = attachment_dir();
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let path = dir.join(format!("{}-{name}", Uuid::new_v4()));
    std::fs::write(&path, &bytes).map_err(|error| error.to_string())?;
    Ok(Attachment {
        id: Uuid::new_v4(),
        name,
        mime_type: mime_type.to_string(),
        size: bytes.len() as u64,
        path: path.to_string_lossy().to_string(),
        data: is_image.then(|| base64::engine::general_purpose::STANDARD.encode(&bytes)),
        is_image,
    })
}

/// Non-image files are stored on disk and shown in the UI; the model does not
/// receive their bytes.
pub fn is_safe_cache_path(path: &Path) -> bool {
    path.starts_with(attachment_dir())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn a_downloaded_image_is_stored_and_inlined() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let address = listener.local_addr().expect("address");
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut buffer = [0u8; 1024];
            let _ = socket.read(&mut buffer).await;
            let body = b"image-bytes";
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: image/png\r\ncontent-length: {}\r\n\r\n",
                body.len()
            );
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.write_all(body).await;
        });
        let attachment = download(
            &reqwest::Client::new(),
            &format!("http://{address}/image.png"),
            None,
            "image.png",
            Some("image/png"),
            Some(11),
        )
        .await
        .expect("attachment");
        assert_eq!(attachment.mime_type, "image/png");
        assert!(attachment.is_image);
        assert!(attachment.data.is_some());
        assert!(std::path::Path::new(&attachment.path).is_file());
        let _ = std::fs::remove_file(&attachment.path);
        task.await.expect("task");
    }

    #[tokio::test]
    async fn an_oversized_attachment_is_rejected_before_download() {
        let error = download(
            &reqwest::Client::new(),
            "http://127.0.0.1:1/never",
            None,
            "big.bin",
            Some("application/octet-stream"),
            Some(MAX_ATTACHMENT_BYTES + 1),
        )
        .await
        .expect_err("oversized");
        assert!(error.contains("20 MB"));
    }
}

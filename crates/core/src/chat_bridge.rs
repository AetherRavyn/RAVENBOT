//! Host bridge for proactive chat messages.
//!
//! The skills crate may not depend on the desktop shell, and the shell owns the
//! connector clients and their keychain-backed credentials. The shell installs
//! one async handler at startup; a skill asks it to send to an already-paired
//! place. A build with no handler fails closed instead of opening a second
//! network stack or reading a token itself.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, OnceLock};
use uuid::Uuid;

/// One outbound message to a paired place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatSendRequest {
    /// The agent asking to post. The shell verifies the link belongs to it.
    pub bot_id: Uuid,
    /// `telegram`, `slack`, `discord` or `whatsapp`.
    pub connection: String,
    /// The platform place id from the link table.
    pub place: String,
    pub text: String,
}

/// A successful send. The handler must not include the token or any secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatSendReceipt {
    pub connection: String,
    pub place: String,
    pub characters: usize,
}

type BoxSendFuture =
    Pin<Box<dyn Future<Output = Result<ChatSendReceipt, String>> + Send + 'static>>;
type Handler = Arc<dyn Fn(ChatSendRequest) -> BoxSendFuture + Send + Sync>;

fn handler_slot() -> &'static Mutex<Option<Handler>> {
    static HANDLER: OnceLock<Mutex<Option<Handler>>> = OnceLock::new();
    HANDLER.get_or_init(|| Mutex::new(None))
}

/// Install the desktop handler. Called once at startup.
pub fn set_chat_send_handler<F, Fut>(handler: F)
where
    F: Fn(ChatSendRequest) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<ChatSendReceipt, String>> + Send + 'static,
{
    let handler: Handler = Arc::new(move |request| Box::pin(handler(request)));
    *handler_slot()
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = Some(handler);
}

/// Is a desktop shell listening for proactive sends?
pub fn chat_send_available() -> bool {
    handler_slot()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .is_some()
}

/// Send through the installed handler. The handler re-checks that the place is
/// paired and that its connector is enabled; this function adds no bypass.
pub async fn send_chat(request: ChatSendRequest) -> Result<ChatSendReceipt, String> {
    let handler = handler_slot()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
        .ok_or_else(|| {
            "Proactive chat messages are only available in the RAVENBOT desktop app.".to_string()
        })?;
    if request.text.trim().is_empty() {
        return Err("The message is empty.".to_string());
    }
    if request.place.trim().is_empty() {
        return Err("The chat place is empty.".to_string());
    }
    handler(request).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_installed_handler_receives_requests_and_rejects_empty_text() {
        set_chat_send_handler(|request| async move {
            Ok(ChatSendReceipt {
                connection: request.connection,
                place: request.place,
                characters: request.text.chars().count(),
            })
        });
        assert!(chat_send_available());
        let receipt = send_chat(ChatSendRequest {
            bot_id: Uuid::new_v4(),
            connection: "telegram".to_string(),
            place: "42".to_string(),
            text: "hello".to_string(),
        })
        .await
        .expect("send");
        assert_eq!(receipt.connection, "telegram");
        assert_eq!(receipt.place, "42");
        assert_eq!(receipt.characters, 5);

        let error = send_chat(ChatSendRequest {
            bot_id: Uuid::new_v4(),
            connection: "telegram".to_string(),
            place: "42".to_string(),
            text: "   ".to_string(),
        })
        .await
        .expect_err("empty");
        assert!(error.contains("empty"));
    }
}

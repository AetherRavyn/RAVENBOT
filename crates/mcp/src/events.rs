//! MCP notifications as routine triggers.
//!
//! MCP has no generic "business event" object. A server pushes JSON-RPC
//! notifications: log messages, resource updates, and catalogue changes. A
//! trigger therefore names a notification method (or `*`), optionally a
//! resource URI, and optionally a substring of the method/URI/params. This
//! module keeps that matching pure so the listener, supervisor and UI tests can
//! all agree on what a trigger means.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// One server-initiated notification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpNotification {
    pub server_id: String,
    /// JSON-RPC method, for example `notifications/message`.
    pub method: String,
    /// The `uri` on a resource notification, when present.
    pub resource_uri: Option<String>,
    pub params: Value,
}

impl McpNotification {
    /// A bounded context block for the routine's first user turn.
    pub fn context(&self, max_chars: usize) -> Value {
        let params = match serde_json::to_string(&self.params) {
            Ok(serialized) if serialized.chars().count() <= max_chars => self.params.clone(),
            _ => Value::String(bounded_json(&self.params, max_chars)),
        };
        json!({
            "kind": "mcp_notification",
            "server": self.server_id,
            "method": self.method,
            "resource_uri": self.resource_uri,
            "params": params,
        })
    }
}

/// Turn a JSON-RPC message into a notification, or `None` for a response or a
/// server request (which carries an id and must be answered, not dispatched).
pub fn parse_notification(server_id: &str, message: &Value) -> Option<McpNotification> {
    let method = message.get("method")?.as_str()?.trim();
    if method.is_empty() || message.get("id").is_some() {
        return None;
    }
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let resource_uri = params
        .get("uri")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|uri| !uri.is_empty())
        .map(str::to_string);
    Some(McpNotification {
        server_id: server_id.to_string(),
        method: method.to_string(),
        resource_uri,
        params,
    })
}

/// Does one configured trigger accept this notification?
///
/// `event` is `*` or an exact JSON-RPC method. `resource_uri` is an exact URI
/// requirement when non-empty. `match_text` is a case-insensitive substring of
/// the method, URI, or serialized params, so `"deploy"` can select the interesting
/// log lines without a second filter language.
pub fn notification_matches(
    event: &str,
    resource_uri: &str,
    match_text: &str,
    notification: &McpNotification,
) -> bool {
    let event = event.trim();
    if event != "*" && !event.eq_ignore_ascii_case(&notification.method) {
        return false;
    }
    let required_uri = resource_uri.trim();
    if !required_uri.is_empty() && notification.resource_uri.as_deref() != Some(required_uri) {
        return false;
    }
    let needle = match_text.trim().to_lowercase();
    if needle.is_empty() {
        return true;
    }
    let haystack = format!(
        "{} {} {}",
        notification.method,
        notification.resource_uri.as_deref().unwrap_or(""),
        notification.params
    )
    .to_lowercase();
    haystack.contains(&needle)
}

fn bounded_json(value: &Value, max_chars: usize) -> String {
    let text = serde_json::to_string(value).unwrap_or_else(|_| "null".to_string());
    if text.chars().count() <= max_chars {
        return text;
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push_str("… [truncated]");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notification(method: &str, params: Value) -> McpNotification {
        parse_notification(
            "github",
            &json!({
                "jsonrpc": "2.0",
                "method": method,
                "params": params,
            }),
        )
        .expect("notification")
    }

    #[test]
    fn responses_and_server_requests_are_not_events() {
        assert!(
            parse_notification("github", &json!({"jsonrpc":"2.0","id":1,"result":{}})).is_none()
        );
        assert!(parse_notification(
            "github",
            &json!({"jsonrpc":"2.0","id":2,"method":"ping","params":{}})
        )
        .is_none());
        assert!(parse_notification("github", &json!({"jsonrpc":"2.0"})).is_none());
    }

    #[test]
    fn a_resource_notification_exposes_its_uri() {
        let event = notification(
            "notifications/resources/updated",
            json!({"uri": "file:///repo/README.md"}),
        );
        assert_eq!(
            event.resource_uri.as_deref(),
            Some("file:///repo/README.md")
        );
    }

    #[test]
    fn an_event_can_match_a_method_wildcard_resource_and_filter() {
        let log = notification(
            "notifications/message",
            json!({"level": "info", "data": "Deploy finished on prod"}),
        );
        assert!(notification_matches("notifications/message", "", "", &log));
        assert!(notification_matches("*", "", "deploy", &log));
        assert!(!notification_matches(
            "notifications/message",
            "",
            "rollback",
            &log
        ));
        assert!(!notification_matches("tools/list_changed", "", "", &log));

        let resource = notification(
            "notifications/resources/updated",
            json!({"uri": "file:///repo/README.md"}),
        );
        assert!(notification_matches(
            "notifications/resources/updated",
            "file:///repo/README.md",
            "",
            &resource
        ));
        assert!(!notification_matches(
            "notifications/resources/updated",
            "file:///other.md",
            "",
            &resource
        ));
        assert!(notification_matches(
            "notifications/resources/updated",
            "file:///repo/README.md",
            "readme",
            &resource
        ));
    }

    #[test]
    fn context_is_bounded_and_named() {
        let event = notification("notifications/message", json!({"data": "x".repeat(10_000)}));
        let context = event.context(500);
        assert_eq!(context["method"], "notifications/message");
        assert_eq!(context["server"], "github");
        let text = context["params"].as_str().expect("serialized params");
        assert!(text.chars().count() <= 520, "{text}");
        assert!(text.ends_with("[truncated]"));

        let small = notification("notifications/message", json!({"data": "short"}));
        assert_eq!(small.context(500)["params"]["data"], "short");
    }
}

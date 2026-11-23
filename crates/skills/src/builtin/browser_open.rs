//! `browser_open` — show a real page in the desktop browser dock.
//!
//! This is deliberately different from `browser_navigate` (headless fetch +
//! readable text). A remote site that sets `X-Frame-Options` cannot be shown in
//! an iframe, so the shell owns a native child webview. The skill asks the shell
//! to open it; headless runs and tests get an honest failure rather than a
//! fabricated success.

use async_trait::async_trait;
use ravenbot_core::Permission;

use crate::traits::{Skill, SkillContext, SkillError, SkillResult, SkillRisk};

pub struct BrowserOpenSkill;

impl BrowserOpenSkill {
    pub fn new() -> Self {
        Self
    }
}

impl Default for BrowserOpenSkill {
    fn default() -> Self {
        Self::new()
    }
}

pub fn validate_http_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("The URL is empty.".to_string());
    }
    let lower = trimmed.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err("Only http:// and https:// URLs can be opened in the dock.".to_string());
    }
    // A URL is passed to a webview, not to a shell, but a control character in
    // a URL is still a sign of intent we should not carry forward.
    if trimmed
        .chars()
        .any(|ch| ch.is_control() || ch.is_whitespace())
    {
        return Err("The URL contains whitespace or control characters.".to_string());
    }
    if trimmed.len() > 8192 {
        return Err("The URL is longer than 8192 characters.".to_string());
    }
    Ok(trimmed.to_string())
}

#[async_trait]
impl Skill for BrowserOpenSkill {
    fn id(&self) -> &str {
        "browser_open"
    }

    fn name(&self) -> &str {
        "Browser Open"
    }

    fn description(&self) -> &str {
        "Show a real web page in the desktop browser dock, where the user can watch, log in, or take over. Use this for any flow that needs a visible page; use browser_navigate for headless research."
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn required_permissions(&self) -> Vec<Permission> {
        vec![Permission::Network {
            domains: vec!["*".to_string()],
        }]
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "http(s) URL to show in the dock"
                },
                "reason": {
                    "type": "string",
                    "description": "Why the user should see this page (e.g. 'log in so I can continue')"
                }
            },
            "required": ["url"]
        })
    }

    fn risk(&self) -> SkillRisk {
        SkillRisk::Low
    }

    async fn execute(
        &self,
        context: &SkillContext,
        arguments: serde_json::Value,
    ) -> Result<SkillResult, SkillError> {
        let raw = arguments
            .get("url")
            .and_then(|value| value.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'url' field".to_string()))?;
        let url = validate_http_url(raw).map_err(SkillError::InvalidArguments)?;

        if !ravenbot_core::browser_dock_available() {
            return Ok(SkillResult::failure(
                "No browser dock is available in this process. Run RAVENBOT as the desktop app to show a page; use browser_navigate for a headless fetch.",
            ));
        }
        let accepted = ravenbot_core::request_browser_dock(url.clone());
        ravenbot_core::emit_agent_activity(
            "browser",
            context.bot_id,
            "open",
            url.clone(),
            accepted,
        );
        if !accepted {
            return Ok(SkillResult::failure(
                "The browser dock refused to open the page. It may not be ready yet.",
            ));
        }
        Ok(SkillResult::success(serde_json::json!({
            "action": "open",
            "url": url,
            "visible": true,
            "hint": "The page is open in the dock. Use browser_dom action=snapshot before clicking or filling."
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_http_urls_are_accepted() {
        assert!(validate_http_url("https://example.com").is_ok());
        assert!(validate_http_url("http://localhost:3000/login").is_ok());
        for bad in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "example.com",
            " ",
            "https://x y",
        ] {
            assert!(validate_http_url(bad).is_err(), "{bad} should be refused");
        }
    }
}

//! Browser automation — real navigation (beats Browserbase paywall, local)

use async_trait::async_trait;
use ravenbot_core::Permission;
use crate::traits::{Skill, SkillContext, SkillError, SkillResult};

pub struct BrowserSkill;

impl BrowserSkill { pub fn new() -> Self { Self } }

#[async_trait]
impl Skill for BrowserSkill {
    fn id(&self) -> &str { "browser_navigate" }
    fn name(&self) -> &str { "Browser Navigate" }
    fn description(&self) -> &str { "Navigate browser, click, fill, screenshot — local computer control" }
    fn version(&self) -> &str { "1.0.0" }
    fn required_permissions(&self) -> Vec<Permission> {
        vec![Permission::Screenshot, Permission::InputControl, Permission::Network { domains: vec!["*".into()] }]
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type":"object","properties":{
                "action":{"type":"string","enum":["navigate","click","fill","screenshot","scroll","wait"],"description":"Browser action"},
                "url":{"type":"string","description":"URL for navigate"},
                "selector":{"type":"string","description":"CSS selector for click/fill"},
                "text":{"type":"string","description":"Text to fill"},
                "x":{"type":"number","description":"X for click if no selector"},
                "y":{"type":"number","description":"Y for click"}
            },"required":["action"]
        })
    }
    async fn execute(&self, _ctx: &SkillContext, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let client = reqwest::Client::new();
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("screenshot");
        match action {
            "navigate" => {
                let url = args.get("url").and_then(|v| v.as_str()).ok_or_else(|| SkillError::InvalidArguments("Missing url".into()))?;
                if !url.starts_with("http://") && !url.starts_with("https://") {
                    return Err(SkillError::InvalidArguments("URL must be http(s)".into()));
                }
                // Real navigation: fetch + readable text extraction (no stub)
                let res = client
                    .get(url)
                    .header("User-Agent", "RAVENBOT/1.0")
                    .send()
                    .await
                    .map_err(|e| SkillError::Network(e.to_string()))?;
                let status = res.status().as_u16();
                let html = res.text().await.map_err(|e| SkillError::Network(e.to_string()))?;
                let (title, text) = extract_readable(&html, 8000);
                Ok(SkillResult::success(serde_json::json!({
                    "action": "navigate",
                    "url": url,
                    "status": status,
                    "title": title,
                    "text": text
                })))
            },
            "click" => {
                let sel = args.get("selector").and_then(|v| v.as_str()).unwrap_or("");
                // Coordinate clicks are real (desktop input injection); a
                // selector-only click needs a DOM we don't own, and is refused
                // honestly rather than reported as success.
                if let (Some(x), Some(y)) = (
                    args.get("x").and_then(|v| v.as_f64()),
                    args.get("y").and_then(|v| v.as_f64()),
                ) {
                    let injector = ravenbot_vision::InputInjector::detect();
                    if !injector.is_available() {
                        return Ok(SkillResult::failure(
                            "No desktop input backend available. Install xdotool (X11) or ydotool/wtype (Wayland), or use the `computer_control` skill.",
                        ));
                    }
                    injector
                        .click(x as f32, y as f32, ravenbot_vision::input::Button::Left)
                        .await
                        .map_err(SkillError::Execution)?;
                    Ok(SkillResult::success(serde_json::json!({
                        "action": "click",
                        "x": x, "y": y,
                        "via": "desktop-input",
                        "backend": injector.backend_summary(),
                    })))
                } else {
                    Ok(SkillResult::failure(format!(
                        "A selector-based click ({:?}) needs a live browser DOM, which is unavailable. Take a screenshot, then click by x/y coordinates, or use the `computer_control` skill.",
                        sel
                    )))
                }
            },
            "fill" => {
                let sel = args.get("selector").and_then(|v| v.as_str()).unwrap_or("");
                // Typing is real; targeting a selector is not.
                if let Some(text) = args.get("text").and_then(|v| v.as_str()) {
                    if sel.is_empty() {
                        let injector = ravenbot_vision::InputInjector::detect();
                        if !injector.is_available() {
                            return Ok(SkillResult::failure(
                                "No desktop input backend available for typing. Install xdotool/ydotool/wtype.",
                            ));
                        }
                        injector.type_text(text).await.map_err(SkillError::Execution)?;
                        return Ok(SkillResult::success(serde_json::json!({
                            "action": "fill",
                            "typed": true,
                            "via": "desktop-input",
                        })));
                    }
                }
                Ok(SkillResult::failure(format!(
                    "Filling the selector {:?} needs a live browser DOM. Click the field by coordinates, then use action=fill with no selector to type into it.",
                    sel
                )))
            },
            "screenshot" => {
                let cap = ravenbot_vision::ScreenshotCapture::new();
                let shot = cap.capture().await.map_err(|e| SkillError::Execution(e.to_string()))?;
                Ok(SkillResult::success(serde_json::json!({"action":"screenshot","format":shot.format,"width":shot.width,"height":shot.height,"data_url": shot.to_data_url()})))
            },
            "scroll" => {
                let injector = ravenbot_vision::InputInjector::detect();
                if !injector.is_available() {
                    return Ok(SkillResult::failure("No desktop input backend available for scrolling."));
                }
                let x = args.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let y = args.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let dy = args.get("delta_y").and_then(|v| v.as_i64()).unwrap_or(3) as i32;
                injector.scroll(x, y, 0, dy).await.map_err(SkillError::Execution)?;
                Ok(SkillResult::success(serde_json::json!({
                    "action": "scroll", "x": x, "y": y, "delta_y": dy, "via": "desktop-input",
                })))
            },
            "wait" => {
                let ms = args.get("milliseconds").and_then(|v| v.as_u64()).unwrap_or(1000).min(30_000);
                tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
                Ok(SkillResult::success(serde_json::json!({"action":"wait","milliseconds":ms})))
            },
            _ => Err(SkillError::InvalidArguments(format!("Unknown browser action: {} (navigate, click, fill, screenshot, scroll, wait)", action)))
        }
    }
}

/// Minimal readable-text extraction: title + tag-stripped text.
fn extract_readable(html: &str, max_chars: usize) -> (String, String) {
    let mut title = String::new();
    if let Some(start) = html.find("<title") {
        if let Some(rest_start) = html[start..].find('>') {
            let rest = &html[start + rest_start + 1..];
            if let Some(end) = rest.find("</title>") {
                title = rest[..end].trim().to_string();
            }
        }
    }

    // Drop script/style blocks (title already extracted), then strip tags
    let mut cleaned = html.to_string();
    for tag in ["script", "style", "noscript", "title"] {
        while let (Some(open), tag_name) = (cleaned.find(&format!("<{}", tag)), tag) {
            let _ = tag_name;
            let Some(rel_close) = cleaned[open..].find(&format!("</{}>", tag)) else {
                break;
            };
            cleaned.replace_range(open..open + rel_close + tag.len() + 3, "");
        }
    }

    let mut text = String::with_capacity(cleaned.len());
    let mut inside_tag = false;
    for ch in cleaned.chars() {
        match ch {
            '<' => inside_tag = true,
            '>' => {
                inside_tag = false;
                // Tags separate text nodes so words don't fuse across them
                text.push(' ');
            }
            c if !inside_tag => text.push(c),
            _ => {}
        }
        if text.len() >= max_chars {
            break;
        }
    }

    // Collapse whitespace
    let collapsed: String = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    (title, collapsed.chars().take(max_chars).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_title_and_text() {
        let (title, text) = extract_readable(
            "<html><head><title>Rust Blog</title></head><body><h1>Hello</h1><p>World</p></body></html>",
            500,
        );
        assert_eq!(title, "Rust Blog");
        assert_eq!(text, "Hello World");
    }

    #[test]
    fn drops_script_and_style() {
        let (_, text) = extract_readable(
            "<body><script>evil()</script><style>x{}</style>content here</body>",
            500,
        );
        assert_eq!(text, "content here");
    }

    #[tokio::test]
    async fn selector_click_is_honestly_refused() {
        use uuid::Uuid;
        let skill = BrowserSkill::new();
        let ctx = crate::traits::SkillContext::with_default_tier(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        );
        // No coordinates, only a selector → must NOT report a fabricated success.
        let result = skill
            .execute(&ctx, serde_json::json!({"action": "click", "selector": "#submit"}))
            .await
            .expect("execute");
        assert!(!result.success);
        assert!(result.error.unwrap_or_default().contains("browser DOM"));
    }

    #[tokio::test]
    async fn coordinate_click_reports_backend_honestly() {
        use uuid::Uuid;
        let skill = BrowserSkill::new();
        let ctx = crate::traits::SkillContext::with_default_tier(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        );
        let result = skill
            .execute(&ctx, serde_json::json!({"action": "click", "x": 10, "y": 20}))
            .await
            .expect("execute");
        // Either it really clicked or it honestly reports no backend — never a
        // fake success without a backend.
        let injector = ravenbot_vision::InputInjector::detect();
        if injector.is_available() {
            assert!(result.success);
        } else {
            assert!(!result.success);
            assert!(result.error.unwrap_or_default().contains("No desktop input backend"));
        }
    }
}

impl Default for BrowserSkill {
    fn default() -> Self {
        Self::new()
    }
}

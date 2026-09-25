//! Computer control skill — drive the real desktop from a running bot.
//!
//! Each call performs one input action (click, type, key, scroll, move) or
//! captures the screen. Actions run through `ravenbot-vision`'s input layer,
//! which detects xdotool/ydotool/wtype and reports honestly when a capability
//! is unavailable. High risk: every action is gated by the approval broker
//! unless the bot is in Full mode.

use async_trait::async_trait;
use ravenbot_core::Permission;
use ravenbot_vision::computer_control::{ComputerAction, MouseButton};
use ravenbot_vision::ComputerController;

use crate::traits::{Skill, SkillContext, SkillError, SkillResult, SkillRisk};

pub struct ComputerControlSkill;

impl ComputerControlSkill {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Skill for ComputerControlSkill {
    fn id(&self) -> &str {
        "computer_control"
    }

    fn name(&self) -> &str {
        "Computer Control"
    }

    fn description(&self) -> &str {
        "Control the real desktop: move the mouse, click, type text, press keys, \
         scroll, or take a screenshot. Use for GUI tasks that no CLI tool covers. \
         Coordinates are in screen pixels."
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn required_permissions(&self) -> Vec<Permission> {
        vec![Permission::InputControl, Permission::Screenshot]
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["click", "double_click", "right_click", "move", "type", "key", "scroll", "screenshot"],
                    "description": "The action to perform"
                },
                "x": { "type": "integer", "description": "Screen X coordinate (pointer actions)" },
                "y": { "type": "integer", "description": "Screen Y coordinate (pointer actions)" },
                "text": { "type": "string", "description": "Text to type (action=type)" },
                "key": { "type": "string", "description": "Key name, e.g. Return, ctrl+c, Tab (action=key)" },
                "delta_x": { "type": "integer", "description": "Horizontal scroll amount (action=scroll)" },
                "delta_y": { "type": "integer", "description": "Vertical scroll amount; positive scrolls down (action=scroll)" }
            },
            "required": ["action"]
        })
    }

    fn risk(&self) -> SkillRisk {
        SkillRisk::High
    }

    async fn execute(
        &self,
        _context: &SkillContext,
        arguments: serde_json::Value,
    ) -> Result<SkillResult, SkillError> {
        let action = arguments
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'action' field".to_string()))?;

        let controller = ComputerController::new();

        // Screenshot is read-only and always available.
        if action == "screenshot" {
            let capture = ravenbot_vision::ScreenshotCapture::new();
            let shot = capture
                .capture()
                .await
                .map_err(|e| SkillError::Execution(e.to_string()))?;
            return Ok(SkillResult::success(serde_json::json!({
                "action": "screenshot",
                "width": shot.width,
                "height": shot.height,
                "data_url": shot.to_data_url(),
            })));
        }

        let num = |key: &str, default: f32| -> f32 {
            arguments
                .get(key)
                .and_then(|v| v.as_f64())
                .map(|v| v as f32)
                .unwrap_or(default)
        };
        let int = |key: &str| -> i32 {
            arguments.get(key).and_then(|v| v.as_i64()).unwrap_or(0) as i32
        };

        let computer_action = match action {
            "click" => ComputerAction::Click { x: num("x", 0.0), y: num("y", 0.0), button: MouseButton::Left },
            "double_click" => ComputerAction::DoubleClick { x: num("x", 0.0), y: num("y", 0.0) },
            "right_click" => ComputerAction::RightClick { x: num("x", 0.0), y: num("y", 0.0) },
            "move" => ComputerAction::MoveMouse { x: num("x", 0.0), y: num("y", 0.0) },
            "type" => ComputerAction::TypeText {
                text: arguments
                    .get("text")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| SkillError::InvalidArguments("action=type requires 'text'".to_string()))?
                    .to_string(),
            },
            "key" => ComputerAction::PressKey {
                key: arguments
                    .get("key")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| SkillError::InvalidArguments("action=key requires 'key'".to_string()))?
                    .to_string(),
            },
            "scroll" => ComputerAction::Scroll {
                x: num("x", 0.0),
                y: num("y", 0.0),
                delta_x: int("delta_x"),
                delta_y: int("delta_y"),
            },
            other => {
                return Ok(SkillResult::failure(format!(
                    "Unknown action '{}'. Valid: click, double_click, right_click, move, type, key, scroll, screenshot.",
                    other
                )))
            }
        };

        if !controller.input_available() {
            return Ok(SkillResult::failure(
                "No desktop input backend is available on this machine. Install `xdotool` (X11) or `ydotool`/`wtype` (Wayland).",
            ));
        }

        let plan = controller
            .input_plan(&computer_action)
            .unwrap_or_else(|| "unavailable".to_string());

        let result = controller
            .execute(&computer_action)
            .await
            .map_err(|e| SkillError::Execution(e.to_string()))?;

        let mut output = serde_json::json!({
            "action": action,
            "success": result.success,
            "message": result.message,
            "backend": controller.input_backend(),
            "plan": plan,
        });
        if let Some(shot) = result.screenshot_after {
            output["screenshot_after"] = serde_json::json!(shot.to_data_url());
        }
        Ok(SkillResult::success(output))
    }
}

impl Default for ComputerControlSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn ctx() -> SkillContext {
        SkillContext::with_default_tier(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4())
    }

    #[tokio::test]
    async fn unknown_action_reports_the_valid_set() {
        let skill = ComputerControlSkill::new();
        let result = skill
            .execute(&ctx(), serde_json::json!({"action": "fly"}))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result
            .error
            .unwrap_or_default()
            .contains("Unknown action"));
    }

    #[tokio::test]
    async fn type_without_text_is_invalid() {
        let skill = ComputerControlSkill::new();
        let err = skill
            .execute(&ctx(), serde_json::json!({"action": "type"}))
            .await
            .unwrap_err();
        assert!(matches!(err, SkillError::InvalidArguments(_)));
    }

    #[tokio::test]
    async fn screenshot_returns_a_real_image_frame() {
        // Headless CI has no display; either a real frame or a descriptive
        // error is acceptable, never a fabricated image.
        let skill = ComputerControlSkill::new();
        let result = skill
            .execute(&ctx(), serde_json::json!({"action": "screenshot"}))
            .await;
        match result {
            Ok(r) => {
                assert!(r.output.get("data_url").is_some());
                assert!(r.output.get("width").is_some());
            }
            Err(e) => assert!(!e.to_string().is_empty()),
        }
    }
}

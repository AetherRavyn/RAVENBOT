//! Screenshot capture skill

use async_trait::async_trait;
use ravenbot_core::Permission;

use crate::traits::{Skill, SkillContext, SkillError, SkillResult};

pub struct ScreenshotSkill;

impl ScreenshotSkill {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Skill for ScreenshotSkill {
    fn id(&self) -> &str {
        "screenshot"
    }

    fn name(&self) -> &str {
        "Screenshot"
    }

    fn description(&self) -> &str {
        "Capture a screenshot of the screen"
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn required_permissions(&self) -> Vec<Permission> {
        vec![Permission::Screenshot]
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "region": {
                    "type": "object",
                    "description": "Optional region to capture (x, y, width, height)",
                    "properties": {
                        "x": { "type": "integer" },
                        "y": { "type": "integer" },
                        "width": { "type": "integer" },
                        "height": { "type": "integer" }
                    }
                }
            }
        })
    }

    async fn execute(
        &self,
        _context: &SkillContext,
        _arguments: serde_json::Value,
    ) -> Result<SkillResult, SkillError> {
        let capture = ravenbot_vision::ScreenshotCapture::new();
        let screenshot = capture.capture().await
            .map_err(|e| SkillError::Execution(e.to_string()))?;

        Ok(SkillResult::success(serde_json::json!({
            "format": screenshot.format,
            "width": screenshot.width,
            "height": screenshot.height,
            "data_url": screenshot.to_data_url(),
            "timestamp": screenshot.timestamp.to_rfc3339()
        })))
    }
}

impl Default for ScreenshotSkill {
    fn default() -> Self {
        Self::new()
    }
}

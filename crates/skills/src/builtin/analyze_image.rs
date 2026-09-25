//! Image analysis skill

use async_trait::async_trait;
use ravenbot_core::Permission;

use crate::traits::{Skill, SkillContext, SkillError, SkillResult, SkillRisk};

pub struct AnalyzeImageSkill;

impl AnalyzeImageSkill {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Skill for AnalyzeImageSkill {
    fn id(&self) -> &str {
        "analyze_image"
    }

    fn name(&self) -> &str {
        "Analyze Image"
    }

    fn description(&self) -> &str {
        "Analyze an image to understand its contents"
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
                "image_data": {
                    "type": "string",
                    "description": "Base64-encoded image data"
                },
                "format": {
                    "type": "string",
                    "description": "Image format (png, jpeg, etc.)",
                    "enum": ["png", "jpeg", "jpg", "gif", "webp"]
                },
                "question": {
                    "type": "string",
                    "description": "Specific question about the image"
                }
            },
            "required": ["image_data"]
        })
    }

    fn risk(&self) -> SkillRisk { SkillRisk::ReadOnly }

    async fn execute(
        &self,
        _context: &SkillContext,
        _arguments: serde_json::Value,
    ) -> Result<SkillResult, SkillError> {
        // Never reached in production: the runtime routes "analyze_image" to
        // exec_analyze_image (it owns the provider manager + DB pool). This
        // path only fires for standalone registry users, who get an honest
        // error instead of canned text.
        Err(SkillError::Execution(
            "analyze_image requires the agent runtime (provider-backed); standalone registry execution is unsupported".to_string(),
        ))
    }
}

impl Default for AnalyzeImageSkill {
    fn default() -> Self {
        Self::new()
    }
}

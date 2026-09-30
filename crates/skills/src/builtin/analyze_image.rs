//! Image analysis skill

use async_trait::async_trait;
use ravenbot_core::Permission;

use crate::traits::{Skill, SkillContext, SkillError, SkillResult};

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

    async fn execute(
        &self,
        _context: &SkillContext,
        arguments: serde_json::Value,
    ) -> Result<SkillResult, SkillError> {
        let image_data = arguments
            .get("image_data")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'image_data' field".to_string()))?;

        let format = arguments
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("png");

        let _question = arguments
            .get("question")
            .and_then(|v| v.as_str())
            .unwrap_or("Describe this image");

        // Decode base64 image
        use base64::Engine;
        let image_bytes = base64::engine::general_purpose::STANDARD
            .decode(image_data)
            .map_err(|e| SkillError::InvalidArguments(format!("Invalid base64: {}", e)))?;

        let analyzer = ravenbot_vision::ImageAnalyzer::new();
        let result = analyzer.analyze_image(&image_bytes, format).await
            .map_err(|e| SkillError::Execution(e.to_string()))?;

        Ok(SkillResult::success(serde_json::json!({
            "description": result.description,
            "elements": result.elements,
            "text_content": result.text_content,
            "confidence": result.confidence
        })))
    }
}

impl Default for AnalyzeImageSkill {
    fn default() -> Self {
        Self::new()
    }
}

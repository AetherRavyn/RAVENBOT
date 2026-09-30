//! Image generation skill
//!
//! Generates images from a text prompt. Uses OpenAI images API when an
//! OPENAI_API_KEY is present, otherwise falls back to Pollinations.ai
//! (free, keyless, privacy-preserving prompt-to-image).

use async_trait::async_trait;
use ravenbot_core::Permission;

use crate::traits::{Skill, SkillContext, SkillError, SkillResult};

pub struct ImageGenSkill {
    client: reqwest::Client,
}

impl ImageGenSkill {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl Default for ImageGenSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Skill for ImageGenSkill {
    fn id(&self) -> &str {
        "image_gen"
    }

    fn name(&self) -> &str {
        "Image Generation"
    }

    fn description(&self) -> &str {
        "Generate an image from a text prompt. Include the returned url in your answer as a markdown image: ![description](url)"
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
                "prompt": {
                    "type": "string",
                    "description": "Detailed description of the image to generate"
                },
                "size": {
                    "type": "string",
                    "description": "OpenAI image size (e.g. 1024x1024). Ignored by the Pollinations fallback",
                    "enum": ["256x256", "512x512", "1024x1024"]
                }
            },
            "required": ["prompt"]
        })
    }

    async fn execute(
        &self,
        _context: &SkillContext,
        arguments: serde_json::Value,
    ) -> Result<SkillResult, SkillError> {
        let prompt = arguments
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'prompt' field".to_string()))?;

        let size = arguments
            .get("size")
            .and_then(|v| v.as_str())
            .unwrap_or("1024x1024");

        // Preferred: OpenAI images API when a key is configured
        if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            if !key.trim().is_empty() {
                let res = self
                    .client
                    .post("https://api.openai.com/v1/images/generations")
                    .bearer_auth(key.trim())
                    .json(&serde_json::json!({
                        "model": "dall-e-3",
                        "prompt": prompt,
                        "n": 1,
                        "size": size,
                        "response_format": "url"
                    }))
                    .send()
                    .await;

                if let Ok(r) = res {
                    if let Ok(j) = r.json::<serde_json::Value>().await {
                        if let Some(url) = j.pointer("/data/0/url").and_then(|v| v.as_str()) {
                            return Ok(SkillResult::success(serde_json::json!({
                                "provider": "openai",
                                "prompt": prompt,
                                "url": url,
                                "markdown": format!("![generated image]({})", url)
                            })));
                        }
                    }
                }
                // fall through to Pollinations if OpenAI path failed
            }
        }

        // Fallback: Pollinations (keyless) — GET the image URL directly
        let encoded = urlencoding::encode(prompt);
        let url = format!("https://image.pollinations.ai/prompt/{}?width=1024&height=1024&nologo=true", encoded);

        // Probe once so failures surface as skill errors rather than broken <img> tags
        let probe = self
            .client
            .head(&url)
            .header("User-Agent", "RAVENBOT/1.0")
            .send()
            .await
            .map_err(|e| SkillError::Network(format!("Image generation failed: {}", e)))?;

        if !probe.status().is_success() {
            return Ok(SkillResult::failure(format!(
                "Image generation service returned status {}",
                probe.status()
            )));
        }

        Ok(SkillResult::success(serde_json::json!({
            "provider": "pollinations",
            "prompt": prompt,
            "url": url,
            "markdown": format!("![generated image]({})", url)
        })))
    }
}

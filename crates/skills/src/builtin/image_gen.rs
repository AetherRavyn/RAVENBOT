//! Image generation skill
//!
//! Generates images from a text prompt. Uses OpenAI images API when an
//! OPENAI_API_KEY is present, otherwise falls back to Pollinations.ai
//! (free, keyless, privacy-preserving prompt-to-image).

use async_trait::async_trait;
use ravenbot_core::Permission;
use uuid::Uuid;

use crate::traits::{Skill, SkillContext, SkillError, SkillResult, SkillRisk};

pub struct ImageGenSkill {
    client: reqwest::Client,
}

struct CachedImage {
    path: String,
    name: String,
    mime: String,
    size: u64,
}

impl ImageGenSkill {
    pub fn new() -> Self {
        Self {
            client: crate::http::http_client(),
        }
    }

    /// Download a generated image once and keep it beside the other cached
    /// artifacts. The model still sees only the URL/path, never a base64 blob;
    /// the runtime turns the local file into a message attachment.
    async fn cache_image(&self, context: &SkillContext, url: &str) -> Result<CachedImage, String> {
        context.ensure_network_allowed(url)?;
        let response = self
            .client
            .get(url)
            .header("User-Agent", "RAVENBOT/1.0")
            .send()
            .await
            .map_err(|error| format!("Image download failed: {error}"))?;
        if !response.status().is_success() {
            return Err(format!(
                "Image service returned status {}",
                response.status()
            ));
        }
        let mime = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("image/png")
            .split(';')
            .next()
            .unwrap_or("image/png")
            .trim()
            .to_string();
        if !mime.starts_with("image/") {
            return Err(format!("Image service returned {mime}, not an image"));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|error| format!("Image download failed: {error}"))?;
        if bytes.is_empty() {
            return Err("Image service returned an empty file".to_string());
        }
        if bytes.len() > 10 * 1024 * 1024 {
            return Err("Generated image is larger than 10 MB".to_string());
        }

        let extension = match mime.as_str() {
            "image/jpeg" => "jpg",
            "image/webp" => "webp",
            "image/gif" => "gif",
            "image/svg+xml" => "svg",
            _ => "png",
        };
        let dir = ravenbot_core::cache_dir().join("generated-images");
        std::fs::create_dir_all(&dir)
            .map_err(|error| format!("Could not create the image cache: {error}"))?;
        let name = format!("generated-{}.{extension}", Uuid::new_v4());
        let path = dir.join(&name);
        std::fs::write(&path, &bytes)
            .map_err(|error| format!("Could not store the generated image: {error}"))?;
        Ok(CachedImage {
            path: path.to_string_lossy().to_string(),
            name,
            mime,
            size: bytes.len() as u64,
        })
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

    fn risk(&self) -> SkillRisk {
        SkillRisk::Low
    }

    async fn execute(
        &self,
        context: &SkillContext,
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

        // Preferred: OpenAI images API when a key is configured. The saved
        // provider key is passed through SkillContext; env remains the
        // headless fallback.
        let openai_key = context
            .openai_api_key
            .clone()
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
            .filter(|key| !key.trim().is_empty());
        if let Some(key) = openai_key {
            context
                .ensure_network_allowed("https://api.openai.com/v1/images/generations")
                .map_err(SkillError::InvalidArguments)?;
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
                        let mut result = serde_json::json!({
                            "provider": "openai",
                            "prompt": prompt,
                            "url": url,
                            "markdown": format!("![generated image]({})", url)
                        });
                        if let Ok(cached) = self.cache_image(context, url).await {
                            result["path"] = serde_json::json!(cached.path);
                            result["name"] = serde_json::json!(cached.name);
                            result["mime"] = serde_json::json!(cached.mime);
                            result["size"] = serde_json::json!(cached.size);
                        }
                        return Ok(SkillResult::success(result));
                    }
                }
            }
            // fall through to Pollinations if OpenAI path failed
        }

        // Fallback: Pollinations (keyless) — GET the image URL directly
        let encoded = urlencoding::encode(prompt);
        let url = format!(
            "https://image.pollinations.ai/prompt/{}?width=1024&height=1024&nologo=true",
            encoded
        );

        let cached = match self.cache_image(context, &url).await {
            Ok(cached) => cached,
            Err(error) => return Ok(SkillResult::failure(error)),
        };

        Ok(SkillResult::success(serde_json::json!({
            "provider": "pollinations",
            "prompt": prompt,
            "url": url,
            "path": cached.path,
            "name": cached.name,
            "mime": cached.mime,
            "size": cached.size,
            "markdown": format!("![generated image]({})", url)
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::header::CONTENT_TYPE;
    use axum::routing::get;
    use axum::Router;

    async fn mock_image() -> impl axum::response::IntoResponse {
        ([(CONTENT_TYPE, "image/png")], b"PNGDATA".to_vec())
    }

    #[tokio::test]
    async fn generated_images_are_cached_with_a_safe_name() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let base = format!("http://{}", listener.local_addr().expect("address"));
        let app = Router::new().route("/image.png", get(mock_image));
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let context =
            SkillContext::with_default_tier(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4())
                .with_private_network(true);
        let skill = ImageGenSkill {
            client: reqwest::Client::new(),
        };
        let cached = skill
            .cache_image(&context, &format!("{base}/image.png"))
            .await
            .expect("cached image");
        assert!(cached.name.starts_with("generated-"));
        assert!(cached.name.ends_with(".png"));
        assert_eq!(cached.mime, "image/png");
        assert_eq!(cached.size, 7);
        assert_eq!(
            std::fs::read(&cached.path).expect("cached bytes"),
            b"PNGDATA"
        );
        let _ = std::fs::remove_file(cached.path);
        server.abort();
    }
}

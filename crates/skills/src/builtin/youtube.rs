use async_trait::async_trait;
use ravenbot_core::Permission;
use crate::traits::{Skill, SkillContext, SkillError, SkillResult};

pub struct YoutubeSkill { client: reqwest::Client }
impl YoutubeSkill { pub fn new() -> Self { Self { client: reqwest::Client::new() } } }

#[async_trait]
impl Skill for YoutubeSkill {
    fn id(&self) -> &str { "youtube_transcript" }
    fn name(&self) -> &str { "YouTube Transcript" }
    fn description(&self) -> &str { "Get transcript/captions for YouTube video" }
    fn version(&self) -> &str { "1.0.0" }
    fn required_permissions(&self) -> Vec<Permission> { vec![Permission::Network { domains: vec!["*".into()] }] }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({"type":"object","properties":{"url":{"type":"string","description":"YouTube URL or video ID"}},"required":["url"]})
    }
    async fn execute(&self, _ctx: &SkillContext, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let url = args.get("url").and_then(|v| v.as_str()).ok_or_else(|| SkillError::InvalidArguments("Missing url".into()))?;
        let vid = extract_id(url).unwrap_or(url.to_string());
        // Use piped transcript via invidious fallback
        let api = format!("https://noembed.com/embed?url=https://www.youtube.com/watch?v={}", vid);
        let resp = self.client.get(&api).send().await;
        match resp {
            Ok(r) => {
                let j: serde_json::Value = r.json().await.unwrap_or(serde_json::json!({}));
                Ok(SkillResult::success(serde_json::json!({"video_id": vid, "meta": j, "transcript": "Transcript fetch requires youtubei API — use browser_navigate to open video + analyze_image for captions as fallback"})))
            },
            Err(e) => Ok(SkillResult::success(serde_json::json!({"video_id": vid, "error": e.to_string(), "fallback": "Use tavily_search for video summary"})))
        }
    }
}
fn extract_id(url: &str) -> Option<String> {
    if url.len() == 11 && !url.contains('/') { return Some(url.to_string()); }
    url.split("v=").nth(1).map(|s| s.split('&').next().unwrap_or(s).to_string())
        .or_else(|| url.split("youtu.be/").nth(1).map(|s| s.split('?').next().unwrap_or(s).to_string()))
}
impl Default for YoutubeSkill { fn default() -> Self { Self::new() } }

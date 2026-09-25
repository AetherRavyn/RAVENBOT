//! Voice output skill (text-to-speech)

use async_trait::async_trait;
use ravenbot_core::Permission;

use crate::traits::{Skill, SkillContext, SkillError, SkillResult, SkillRisk};

pub struct VoiceOutputSkill;

impl VoiceOutputSkill {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Skill for VoiceOutputSkill {
    fn id(&self) -> &str {
        "voice_output"
    }

    fn name(&self) -> &str {
        "Voice Output"
    }

    fn description(&self) -> &str {
        "Convert text to speech"
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn required_permissions(&self) -> Vec<Permission> {
        vec![Permission::AudioPlayback]
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "Text to speak"
                },
                "voice": {
                    "type": "string",
                    "description": "Voice name (optional)"
                },
                "rate": {
                    "type": "number",
                    "description": "Speaking rate (0.5-2.0, default: 1.0)"
                }
            },
            "required": ["text"]
        })
    }

    fn risk(&self) -> SkillRisk { SkillRisk::Low }

    async fn execute(
        &self,
        _context: &SkillContext,
        arguments: serde_json::Value,
    ) -> Result<SkillResult, SkillError> {
        let text = arguments
            .get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'text' field".to_string()))?;

        let voice = arguments
            .get("voice")
            .and_then(|v| v.as_str())
            .unwrap_or("default");

        let rate = arguments
            .get("rate")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0) as f32;

        let tts = ravenbot_vision::TextToSpeech::new()
            .with_voice(ravenbot_vision::audio::tts::VoiceOptions {
                voice: voice.to_string(),
                rate,
                ..Default::default()
            });

        let audio = tts.synthesize(text).await
            .map_err(|e| SkillError::Execution(e.to_string()))?;

        // Return base64 audio so the UI can play it via Web Audio API.
        use base64::Engine;
        let audio_b64 = base64::engine::general_purpose::STANDARD.encode(&audio);

        Ok(SkillResult::success(serde_json::json!({
            "text": text,
            "voice": voice,
            "rate": rate,
            "audio_size": audio.len(),
            "audio_b64": audio_b64,
            "format": "wav",
            "note": "Play audio_b64 via the Web Audio API"
        })))
    }
}

impl Default for VoiceOutputSkill {
    fn default() -> Self {
        Self::new()
    }
}

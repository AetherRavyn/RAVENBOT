//! Voice input skill (transcription)

use async_trait::async_trait;
use ravenbot_core::Permission;

use crate::traits::{Skill, SkillContext, SkillError, SkillResult};

pub struct VoiceInputSkill;

impl VoiceInputSkill {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Skill for VoiceInputSkill {
    fn id(&self) -> &str {
        "voice_input"
    }

    fn name(&self) -> &str {
        "Voice Input"
    }

    fn description(&self) -> &str {
        "Transcribe audio to text"
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn required_permissions(&self) -> Vec<Permission> {
        vec![Permission::AudioCapture]
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "audio_data": {
                    "type": "string",
                    "description": "Base64-encoded audio data"
                },
                "format": {
                    "type": "string",
                    "description": "Audio format (wav, mp3, etc.)",
                    "enum": ["wav", "mp3", "ogg", "m4a", "webm"]
                },
                "language": {
                    "type": "string",
                    "description": "Expected language code (optional)"
                }
            },
            "required": ["audio_data"]
        })
    }

    async fn execute(
        &self,
        _context: &SkillContext,
        arguments: serde_json::Value,
    ) -> Result<SkillResult, SkillError> {
        let audio_data = arguments
            .get("audio_data")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'audio_data' field".to_string()))?;

        let format = arguments
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("wav");

        // Decode base64 audio
        use base64::Engine;
        let audio_bytes = base64::engine::general_purpose::STANDARD
            .decode(audio_data)
            .map_err(|e| SkillError::InvalidArguments(format!("Invalid base64: {}", e)))?;

        let transcriber = ravenbot_vision::AudioTranscriber::new();
        let result = transcriber.transcribe(&audio_bytes, format).await
            .map_err(|e| SkillError::Execution(e.to_string()))?;

        Ok(SkillResult::success(serde_json::json!({
            "text": result.text,
            "language": result.language,
            "confidence": result.confidence,
            "duration_secs": result.duration_secs
        })))
    }
}

impl Default for VoiceInputSkill {
    fn default() -> Self {
        Self::new()
    }
}

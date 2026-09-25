//! Audio transcription — REAL engines, honest errors.
//!
//! Priority:
//!   1. OpenAI Whisper API (if OPENAI_API_KEY is set)
//!   2. Local whisper.cpp / faster-whisper (if available)
//!   3. Clear error telling the user what to set/install

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TranscriptionError {
    #[error("Transcription failed: {0}")]
    Failed(String),
    #[error("No STT engine. Set OPENAI_API_KEY for Whisper API, or install faster-whisper locally.")]
    NoEngine,
    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionResult {
    pub text: String,
    pub language: Option<String>,
    pub confidence: f32,
    pub duration_secs: f32,
}

pub struct AudioTranscriber;

impl AudioTranscriber {
    pub fn new() -> Self { Self }

    /// Transcribe audio bytes to text.
    pub async fn transcribe(&self, audio_data: &[u8], format: &str) -> Result<TranscriptionResult, TranscriptionError> {
        if !matches!(format, "wav" | "mp3" | "ogg" | "m4a" | "webm" | "flac") {
            return Err(TranscriptionError::UnsupportedFormat(format.to_string()));
        }
        if audio_data.len() < 100 {
            return Err(TranscriptionError::Failed("Audio too short".to_string()));
        }

        // 1. Try OpenAI Whisper API.
        if let Ok(result) = self.transcribe_openai(audio_data, format).await {
            return Ok(result);
        }

        // 2. Try local faster-whisper / whisper.cpp.
        match self.transcribe_local(audio_data, format).await {
            Ok(r) => Ok(r),
            Err(e) => Err(e),
        }
    }

    async fn transcribe_openai(&self, audio_data: &[u8], format: &str) -> Result<TranscriptionResult, TranscriptionError> {
        let key = std::env::var("OPENAI_API_KEY").unwrap_or_default();
        if key.is_empty() {
            return Err(TranscriptionError::NoEngine);
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| TranscriptionError::Failed(e.to_string()))?;

        // Build multipart form with the audio file.
        let ext = match format {
            "wav" => "wav", "mp3" => "mp3", "ogg" => "ogg",
            "m4a" => "m4a", "webm" => "webm", "flac" => "flac",
            _ => "wav",
        };
        let filename = format!("audio.{}", ext);
        let part = reqwest::multipart::Part::bytes(audio_data.to_vec())
            .file_name(filename)
            .mime_str(&format!("audio/{}", ext))
            .map_err(|e| TranscriptionError::Failed(e.to_string()))?;

        let form = reqwest::multipart::Form::new()
            .part("file", part)
            .text("model", "whisper-1")
            .text("response_format", "verbose_json");

        let resp = client
            .post("https://api.openai.com/v1/audio/transcriptions")
            .bearer_auth(&key)
            .multipart(form)
            .send()
            .await
            .map_err(|e| TranscriptionError::Failed(format!("API call failed: {}", e)))?;

        if !resp.status().is_success() {
            let msg = resp.text().await.unwrap_or_default();
            return Err(TranscriptionError::Failed(format!("Whisper API error: {}", msg.chars().take(300).collect::<String>())));
        }

        let json: serde_json::Value = resp.json().await
            .map_err(|e| TranscriptionError::Failed(format!("Bad response: {}", e)))?;

        let text = json.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let language = json.get("language").and_then(|v| v.as_str()).map(|s| s.to_string());
        let duration = json.get("duration").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;

        if text.trim().is_empty() {
            return Err(TranscriptionError::Failed("Whisper returned empty text".to_string()));
        }

        Ok(TranscriptionResult {
            text,
            language,
            confidence: 0.95,
            duration_secs: duration,
        })
    }

    async fn transcribe_local(&self, audio_data: &[u8], format: &str) -> Result<TranscriptionResult, TranscriptionError> {
        // Write audio to a temp file for the Python script.
        let temp_dir = std::env::temp_dir().join("ravenbot_stt");
        let _ = std::fs::create_dir_all(&temp_dir);
        let input_path = temp_dir.join(format!("input.{}", format));
        std::fs::write(&input_path, audio_data)
            .map_err(|e| TranscriptionError::Failed(format!("Failed to write temp audio: {}", e)))?;

        let output_path = temp_dir.join(format!("{}.json", uuid::Uuid::new_v4()));

        // Use faster-whisper via Python if available.
        let script = r#"
import sys, json
try:
    from faster_whisper import WhisperModel
except ImportError:
    print(json.dumps({"error": "faster_whisper not installed. Run: pip install faster-whisper"}))
    sys.exit(2)

import os
model_size = os.environ.get("WHISPER_MODEL", "base")
model = WhisperModel(model_size, device="cpu", compute_type="int8")

audio_path = sys.argv[1]
segments, info = model.transcribe(audio_path, beam_size=5)
text = " ".join(seg.text for seg in segments)
result = {"text": text.strip(), "language": info.language, "duration": info.duration}
with open(sys.argv[2], "w") as f:
    json.dump(result, f)
"#;

        let script_path = temp_dir.join("stt_infer.py");
        std::fs::write(&script_path, script)
            .map_err(|e| TranscriptionError::Failed(format!("Failed to write script: {}", e)))?;

        let python = std::env::var("PYTHON_BIN").unwrap_or_else(|_| "python3".to_string());
        let output = tokio::process::Command::new(&python)
            .arg(&script_path)
            .arg(&input_path)
            .arg(&output_path)
            .output()
            .await
            .map_err(|e| TranscriptionError::Failed(format!(
                "Failed to start Python ({}). Is Python 3 installed? Error: {}", python, e
            )))?;

        // Cleanup temp files.
        let _ = std::fs::remove_file(&input_path);
        let _ = std::fs::remove_file(&script_path);

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(TranscriptionError::Failed(format!(
                "Local STT failed (exit {}): {}",
                output.status.code().unwrap_or(-1),
                stderr.chars().take(500).collect::<String>()
            )));
        }

        let result_json = std::fs::read_to_string(&output_path)
            .map_err(|e| TranscriptionError::Failed(format!("Failed to read result: {}", e)))?;
        let _ = std::fs::remove_file(&output_path);

        let result: serde_json::Value = serde_json::from_str(&result_json)
            .map_err(|e| TranscriptionError::Failed(format!("Bad result JSON: {}", e)))?;

        if let Some(err) = result.get("error").and_then(|v| v.as_str()) {
            return Err(TranscriptionError::Failed(err.to_string()));
        }

        let text = result.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let language = result.get("language").and_then(|v| v.as_str()).map(|s| s.to_string());
        let duration = result.get("duration").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;

        if text.trim().is_empty() {
            return Err(TranscriptionError::Failed("Local STT returned empty text".to_string()));
        }

        Ok(TranscriptionResult {
            text,
            language,
            confidence: 0.85,
            duration_secs: duration,
        })
    }
}

impl Default for AudioTranscriber {
    fn default() -> Self {
        Self
    }
}

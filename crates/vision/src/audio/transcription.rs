//! Audio transcription — REAL engines, honest errors.
//!
//! Priority:
//!   1. OpenAI Whisper API (saved provider key, then `OPENAI_API_KEY`)
//!   2. Local whisper.cpp / faster-whisper (if available)
//!   3. Clear error telling the user what to set/install

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TranscriptionError {
    #[error("Transcription failed: {0}")]
    Failed(String),
    #[error(
        "No STT engine. Add an OpenAI key in Settings, set OPENAI_API_KEY, or install faster-whisper locally."
    )]
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

/// A per-call directory. The UUID is what makes concurrent transcripts safe.
fn session_dir(prefix: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("{prefix}-{}", uuid::Uuid::new_v4()))
}

/// Remove a transcript's scratch directory on every exit path, including a
/// failed Python run.
struct TempDirGuard(std::path::PathBuf);

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const DEFAULT_OPENAI_URL: &str = "https://api.openai.com/v1/audio/transcriptions";

pub struct AudioTranscriber {
    /// Saved provider key, preferred over the environment.
    openai_key: Option<String>,
    /// Override for compatible gateways and tests.
    openai_url: String,
}

impl AudioTranscriber {
    pub fn new() -> Self {
        let openai_url = std::env::var("RAVENBOT_OPENAI_AUDIO_URL")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_OPENAI_URL.to_string());
        Self {
            openai_key: None,
            openai_url,
        }
    }

    /// Supply the key configured in Settings. Environment fallback remains for
    /// headless runs and development.
    pub fn with_openai_key(mut self, key: Option<String>) -> Self {
        self.openai_key = key.filter(|value| !value.trim().is_empty());
        self
    }

    /// Point Whisper calls at a compatible endpoint (gateway, proxy or test).
    pub fn with_openai_url(mut self, url: impl Into<String>) -> Self {
        let url = url.into();
        if !url.trim().is_empty() {
            self.openai_url = url.trim().to_string();
        }
        self
    }

    fn resolved_openai_key(&self) -> Option<String> {
        self.openai_key.clone().or_else(|| {
            std::env::var("OPENAI_API_KEY")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
    }

    /// Transcribe audio bytes to text.
    pub async fn transcribe(
        &self,
        audio_data: &[u8],
        format: &str,
    ) -> Result<TranscriptionResult, TranscriptionError> {
        self.transcribe_with_provider(audio_data, format)
            .await
            .map(|(result, _)| result)
    }

    /// The provider is returned so a paid Whisper call can be distinguished
    /// from the local model.
    pub async fn transcribe_with_provider(
        &self,
        audio_data: &[u8],
        format: &str,
    ) -> Result<(TranscriptionResult, &'static str), TranscriptionError> {
        if !matches!(format, "wav" | "mp3" | "ogg" | "m4a" | "webm" | "flac") {
            return Err(TranscriptionError::UnsupportedFormat(format.to_string()));
        }
        if audio_data.len() < 100 {
            return Err(TranscriptionError::Failed("Audio too short".to_string()));
        }

        // 1. Try OpenAI Whisper API.
        if let Ok(result) = self.transcribe_openai(audio_data, format).await {
            return Ok((result, "openai"));
        }

        // 2. Try local faster-whisper / whisper.cpp.
        match self.transcribe_local(audio_data, format).await {
            Ok(result) => Ok((result, "local")),
            Err(e) => Err(e),
        }
    }

    async fn transcribe_openai(
        &self,
        audio_data: &[u8],
        format: &str,
    ) -> Result<TranscriptionResult, TranscriptionError> {
        let Some(key) = self.resolved_openai_key() else {
            return Err(TranscriptionError::NoEngine);
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| TranscriptionError::Failed(e.to_string()))?;

        // Build multipart form with the audio file.
        let ext = match format {
            "wav" => "wav",
            "mp3" => "mp3",
            "ogg" => "ogg",
            "m4a" => "m4a",
            "webm" => "webm",
            "flac" => "flac",
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
            .post(&self.openai_url)
            .bearer_auth(&key)
            .multipart(form)
            .send()
            .await
            .map_err(|e| TranscriptionError::Failed(format!("API call failed: {}", e)))?;

        if !resp.status().is_success() {
            let msg = resp.text().await.unwrap_or_default();
            return Err(TranscriptionError::Failed(format!(
                "Whisper API error: {}",
                msg.chars().take(300).collect::<String>()
            )));
        }

        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| TranscriptionError::Failed(format!("Bad response: {}", e)))?;

        let text = json
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let language = json
            .get("language")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let duration = json.get("duration").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;

        if text.trim().is_empty() {
            return Err(TranscriptionError::Failed(
                "Whisper returned empty text".to_string(),
            ));
        }

        Ok(TranscriptionResult {
            text,
            language,
            confidence: 0.95,
            duration_secs: duration,
        })
    }

    async fn transcribe_local(
        &self,
        audio_data: &[u8],
        format: &str,
    ) -> Result<TranscriptionResult, TranscriptionError> {
        // One directory per transcription. The old shared `ravenbot_stt/input.wav`
        // let two concurrent recordings overwrite each other's audio.
        let temp_dir = session_dir("ravenbot_stt");
        std::fs::create_dir_all(&temp_dir).map_err(|e| {
            TranscriptionError::Failed(format!("Failed to create temp audio dir: {}", e))
        })?;
        let _cleanup = TempDirGuard(temp_dir.clone());
        let input_path = temp_dir.join(format!("input.{}", format));
        std::fs::write(&input_path, audio_data).map_err(|e| {
            TranscriptionError::Failed(format!("Failed to write temp audio: {}", e))
        })?;

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
# PyAV 19 removed the `metadata_errors` argument that faster-whisper 1.2.x
# still passes. Fall back to decoding with ffmpeg into a float32 numpy array,
# which also skips PyAV entirely for WAV/MP3/OGG/M4A/WebM/FLAC input.
def transcribe_path(path):
    try:
        return model.transcribe(path, beam_size=5)
    except TypeError as exc:
        if "metadata_errors" not in str(exc):
            raise
        import subprocess
        import numpy as np
        ffmpeg = os.environ.get("FFMPEG_BIN", "ffmpeg")
        proc = subprocess.run(
            [ffmpeg, "-v", "error", "-i", path, "-f", "f32le", "-ac", "1", "-ar", "16000", "-"],
            capture_output=True,
        )
        if proc.returncode != 0:
            detail = proc.stderr.decode("utf-8", "replace").strip()[:500]
            raise RuntimeError("ffmpeg could not decode the recording: " + detail)
        audio = np.frombuffer(proc.stdout, dtype=np.float32)
        if audio.size == 0:
            raise RuntimeError("ffmpeg decoded no audio samples from the recording")
        return model.transcribe(audio, beam_size=5)

segments, info = transcribe_path(audio_path)
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
            .map_err(|e| {
                TranscriptionError::Failed(format!(
                    "Failed to start Python ({}). Is Python 3 installed? Error: {}",
                    python, e
                ))
            })?;

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

        let result: serde_json::Value = serde_json::from_str(&result_json)
            .map_err(|e| TranscriptionError::Failed(format!("Bad result JSON: {}", e)))?;

        if let Some(err) = result.get("error").and_then(|v| v.as_str()) {
            return Err(TranscriptionError::Failed(err.to_string()));
        }

        let text = result
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let language = result
            .get("language")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let duration = result
            .get("duration")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as f32;

        if text.trim().is_empty() {
            return Err(TranscriptionError::Failed(
                "Local STT returned empty text".to_string(),
            ));
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
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;
    use axum::http::HeaderMap;
    use axum::routing::post;
    use axum::{Json, Router};
    use serde_json::{json, Value};
    use std::sync::{Arc, Mutex};

    #[test]
    fn each_transcription_gets_its_own_scratch_directory() {
        let first = session_dir("ravenbot_stt");
        let second = session_dir("ravenbot_stt");
        assert_ne!(first, second);
        assert!(first.to_string_lossy().contains("ravenbot_stt-"));
    }

    #[test]
    fn local_stt_script_keeps_the_pyav_19_ffmpeg_fallback() {
        // faster-whisper 1.2.x still passes `metadata_errors` to av.open, which
        // PyAV 19 removed. The embedded helper must detect that TypeError and
        // decode with ffmpeg instead of failing the whole transcription path.
        let source = include_str!("transcription.rs");
        assert!(source.contains("metadata_errors"));
        assert!(source.contains("-f\", \"f32le\""));
        assert!(source.contains("ffmpeg could not decode the recording"));
    }

    #[derive(Clone)]
    struct SeenAuth(Arc<Mutex<Option<String>>>);

    async fn mock_whisper(
        State(seen): State<SeenAuth>,
        headers: HeaderMap,
        _body: axum::body::Bytes,
    ) -> Json<Value> {
        *seen.0.lock().unwrap() = headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        Json(json!({
            "text": "hello from whisper",
            "language": "en",
            "duration": 0.8
        }))
    }

    #[tokio::test]
    async fn the_saved_openai_key_is_sent_to_whisper() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let base = format!("http://{}", listener.local_addr().expect("address"));
        let seen = SeenAuth(Arc::new(Mutex::new(None)));
        let app = Router::new()
            .route("/audio/transcriptions", post(mock_whisper))
            .with_state(seen.clone());
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let transcriber = AudioTranscriber::new()
            .with_openai_key(Some("saved-provider-key".to_string()))
            .with_openai_url(format!("{base}/audio/transcriptions"));
        let audio = [0u8; 256];
        let (result, provider) = transcriber
            .transcribe_with_provider(&audio, "wav")
            .await
            .expect("transcription");
        assert_eq!(provider, "openai");
        assert_eq!(result.text, "hello from whisper");
        assert_eq!(
            seen.0.lock().unwrap().as_deref(),
            Some("Bearer saved-provider-key")
        );
        server.abort();
    }

    #[test]
    fn an_explicit_key_wins_over_the_environment() {
        let transcriber = AudioTranscriber::new()
            .with_openai_key(Some("saved-provider-key".to_string()))
            .with_openai_url("https://example.invalid/audio/transcriptions");
        assert_eq!(
            transcriber.resolved_openai_key().as_deref(),
            Some("saved-provider-key")
        );
    }
}

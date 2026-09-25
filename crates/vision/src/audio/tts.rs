//! Text-to-speech synthesis — REAL engines, honest errors.
//!
//! Priority:
//!   1. Hugging Face Inference API (if the model has a hosted endpoint)
//!   2. Local Python inference with the downloaded SanoTTTS model
//!   3. Clear error telling the user exactly what to install/set

use base64::{Engine as _, engine::general_purpose};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TtsError {
    #[error("TTS failed: {0}")]
    Failed(String),
    #[error("No TTS engine available. Install PyTorch + transformers in your Python environment, or set HUGGINGFACE_INFERENCE_TOKEN for cloud inference.")]
    NoEngine,
    #[error("Model not downloaded. Run a TTS action once to download the voice model (~几百 MB).")]
    ModelNotReady,
    #[error("Audio output failed: {0}")]
    OutputFailed(String),
}

/// TTS voice options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceOptions {
    pub voice: String,
    pub rate: f32,
    pub volume: f32,
    pub pitch: f32,
}

impl Default for VoiceOptions {
    fn default() -> Self {
        Self { voice: "default".to_string(), rate: 1.0, volume: 1.0, pitch: 1.0 }
    }
}

/// The SanoTTTS model repo on Hugging Face.
const SANO_TTS_REPO: &str = "swadhin234/sanoTTS-bucket";

pub struct TextToSpeech {
    default_options: VoiceOptions,
    /// Optional OpenAI API key for the multi-voice TTS engine (per-bot voices).
    openai_key: Option<String>,
}

impl TextToSpeech {
    pub fn new() -> Self {
        Self { default_options: VoiceOptions::default(), openai_key: None }
    }

    pub fn with_voice(mut self, options: VoiceOptions) -> Self {
        self.default_options = options;
        self
    }

    /// Supply an OpenAI API key so the multi-voice engine can be used.
    pub fn with_openai_key(mut self, key: Option<String>) -> Self {
        self.openai_key = key.filter(|k| !k.trim().is_empty());
        self
    }

    fn resolved_openai_key(&self) -> Option<String> {
        self.openai_key
            .clone()
            .or_else(|| std::env::var("OPENAI_API_KEY").ok().filter(|k| !k.trim().is_empty()))
    }

    /// Synthesize text to WAV bytes. Tries OpenAI (multi-voice) first when a
    /// key is available, then the HF Inference API, then local Python.
    pub async fn synthesize(&self, text: &str) -> Result<Vec<u8>, TtsError> {
        if text.trim().is_empty() {
            return Err(TtsError::Failed("Empty text".to_string()));
        }

        // 1. OpenAI speech (supports distinct voices per bot).
        if self.resolved_openai_key().is_some() {
            if let Ok(audio) = self.synthesize_openai(text).await {
                if !audio.is_empty() {
                    return Ok(audio);
                }
            }
        }

        // 2. Hugging Face Inference API (fastest, no local GPU needed).
        if let Ok(audio) = self.synthesize_cloud(text).await {
            if !audio.is_empty() {
                return Ok(audio);
            }
        }

        // 3. Fall back to local Python inference with the downloaded model.
        match self.synthesize_local(text).await {
            Ok(audio) if !audio.is_empty() => Ok(audio),
            Ok(_) => Err(TtsError::NoEngine),
            Err(e) => Err(e),
        }
    }

    /// OpenAI `/v1/audio/speech` — honours the selected voice and rate.
    async fn synthesize_openai(&self, text: &str) -> Result<Vec<u8>, TtsError> {
        let key = self
            .resolved_openai_key()
            .ok_or_else(|| TtsError::Failed("No OpenAI key".to_string()))?;
        let voice = normalize_openai_voice(&self.default_options.voice);
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(90))
            .build()
            .map_err(|e| TtsError::Failed(e.to_string()))?;

        let resp = client
            .post("https://api.openai.com/v1/audio/speech")
            .bearer_auth(&key)
            .json(&serde_json::json!({
                "model": "tts-1",
                "voice": voice,
                "input": text,
                "response_format": "wav",
                "speed": self.default_options.rate.clamp(0.5, 2.0),
            }))
            .send()
            .await
            .map_err(|e| TtsError::Failed(format!("OpenAI TTS request failed: {}", e)))?;

        if !resp.status().is_success() {
            let msg = resp.text().await.unwrap_or_default();
            return Err(TtsError::Failed(format!(
                "OpenAI TTS error: {}",
                msg.chars().take(300).collect::<String>()
            )));
        }

        resp.bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(|e| TtsError::Failed(format!("Failed to read OpenAI audio: {}", e)))
    }

    /// Hugging Face Inference API — requires a token with inference access.
    async fn synthesize_cloud(&self, text: &str) -> Result<Vec<u8>, TtsError> {
        let token = std::env::var("HUGGINGFACE_INFERENCE_TOKEN")
            .or_else(|_| std::env::var("HF_TOKEN"))
            .unwrap_or_default();
        if token.is_empty() {
            return Err(TtsError::Failed("No HF inference token".to_string()));
        }

        let url = format!(
            "https://api-inference.huggingface.co/models/{}",
            SANO_TTS_REPO
        );
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| TtsError::Failed(e.to_string()))?;

        let resp = client
            .post(&url)
            .bearer_auth(&token)
            .json(&serde_json::json!({ "inputs": text }))
            .send()
            .await
            .map_err(|e| TtsError::Failed(format!("Inference API call failed: {}", e)))?;

        if !resp.status().is_success() {
            let msg = resp.text().await.unwrap_or_default();
            return Err(TtsError::Failed(format!("Inference API error: {}", msg.chars().take(300).collect::<String>())));
        }

        let bytes = resp.bytes().await
            .map_err(|e| TtsError::Failed(format!("Failed to read audio: {}", e)))?;

        // HF returns audio as a JSON with base64 or raw WAV/FLAC bytes.
        // Try to detect: if it starts with RIFF/WAVE or ID3, it's raw audio.
        let is_raw_audio = bytes.len() > 4 && (
            bytes.starts_with(b"RIFF") || bytes.starts_with(b"ID3\x03") ||
            bytes.starts_with(b"fLaC") || bytes.starts_with(b"OggS")
        );

        if is_raw_audio {
            Ok(bytes.to_vec())
        } else {
            // Try parsing as JSON with a base64 audio field.
            if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                if let Some(b64) = val.get("audio").and_then(|v| v.as_str())
                    .or_else(|| val.as_str())
                {
                    return general_purpose::STANDARD.decode(b64).map_err(|e| TtsError::Failed(format!("Bad base64 audio: {}", e)))
                        .map_err(|e| TtsError::Failed(format!("Bad base64 audio: {}", e)));
                }
            }
            // Fallback: assume the bytes ARE the audio (some endpoints return raw).
            if bytes.len() > 100 {
                Ok(bytes.to_vec())
            } else {
                Err(TtsError::Failed("Inference API returned no usable audio".to_string()))
            }
        }
    }

    /// Local Python inference — downloads the model on first use.
    async fn synthesize_local(&self, text: &str) -> Result<Vec<u8>, TtsError> {
        use crate::audio::model_manager::{self, ModelError};

        // Ensure model is cached (downloads on first call).
        let model_dir = if model_manager::is_cached(SANO_TTS_REPO) {
            model_manager::model_dir(SANO_TTS_REPO)
        } else {
            tracing::info!("Downloading SanoTTTS model on first use...");
            model_manager::download_model(SANO_TTS_REPO).await
                .map_err(|e| match e {
                    ModelError::DownloadFailed(msg) => TtsError::Failed(format!("Model download failed: {}", msg)),
                    ModelError::NotFound(f) => TtsError::Failed(format!("Model file missing: {}", f)),
                    ModelError::Io(e) => TtsError::Failed(format!("IO error: {}", e)),
                })?
        };

        // Find the Python inference script.
        // Scripts live at the workspace root, two levels up from the crate.
        let script_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../scripts/infer_sano_tts.py");
        if !script_path.exists() {
            return Err(TtsError::Failed(
                format!("Inference script not found at {}", script_path.display())
            ));
        }

        // Write output to a temp file.
        let output_dir = std::env::temp_dir().join("ravenbot_tts");
        let _ = std::fs::create_dir_all(&output_dir);
        let output_path = output_dir.join(format!("{}.wav", uuid::Uuid::new_v4()));

        // Spawn the Python subprocess.
        let python = std::env::var("PYTHON_BIN").unwrap_or_else(|_| "python3".to_string());
        let output = tokio::process::Command::new(&python)
            .arg(&script_path)
            .arg("--model-dir").arg(&model_dir)
            .arg("--text").arg(text)
            .arg("--output").arg(&output_path)
            .output()
            .await
            .map_err(|e| TtsError::Failed(format!(
                "Failed to start Python ({}). Is Python 3 installed? Error: {}", python, e
            )))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(TtsError::Failed(format!(
                "Python inference failed (exit {}): {}",
                output.status.code().unwrap_or(-1),
                stderr.chars().take(500).collect::<String>()
            )));
        }

        let audio = std::fs::read(&output_path)
            .map_err(|e| TtsError::Failed(format!("Failed to read output audio: {}", e)))?;

        let _ = std::fs::remove_file(&output_path);

        if audio.len() < 100 {
            return Err(TtsError::Failed("Python returned empty/too-short audio".to_string()));
        }

        Ok(audio)
    }

    /// Play audio bytes (platform-specific; best-effort).
    pub async fn play(&self, audio_data: &[u8]) -> Result<(), TtsError> {
        if audio_data.is_empty() {
            return Ok(());
        }
        tracing::info!(size = audio_data.len(), "Playing audio");
        // In a full implementation, route to platform audio (cpal, rodio, or
        // Tauri's webview audio). For now, the UI layer handles playback via
        // the Web Audio API from the returned bytes.
        Ok(())
    }

    pub async fn speak(&self, text: &str) -> Result<Vec<u8>, TtsError> {
        self.synthesize(text).await
    }
}

impl Default for TextToSpeech {
    fn default() -> Self {
        Self::new()
    }
}

/// Voices exposed by the multi-voice (OpenAI) TTS engine.
pub const OPENAI_VOICES: &[&str] = &[
    "alloy", "ash", "ballad", "coral", "echo", "fable", "nova", "onyx", "sage", "shimmer", "verse",
];

/// Map a requested voice id to a supported OpenAI voice (defaults to alloy).
fn normalize_openai_voice(voice: &str) -> &'static str {
    let v = voice.trim().to_lowercase();
    OPENAI_VOICES
        .iter()
        .copied()
        .find(|c| *c == v.as_str())
        .unwrap_or("alloy")
}

//! Text-to-speech synthesis — REAL engines, honest errors.
//!
//! Priority:
//!   1. OpenAI speech (when a key is configured)
//!   2. Hugging Face Inference API (if the model has a hosted endpoint)
//!   3. Piper, a free local ONNX engine (when installed)
//!   4. Local Python inference with the downloaded SanoTTTS model
//!   5. Clear error telling the user exactly what to install/set

use base64::{engine::general_purpose, Engine as _};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use thiserror::Error;
use tokio::io::AsyncWriteExt;

/// The default Piper voice downloaded by the documented setup command.
pub const PIPER_DEFAULT_VOICE: &str = "en_US-lessac-medium";

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
        Self {
            voice: "default".to_string(),
            rate: 1.0,
            volume: 1.0,
            pitch: 1.0,
        }
    }
}

/// The SanoTTTS model repo on Hugging Face.
const SANO_TTS_REPO: &str = "swadhin234/sanoTTS-bucket";

const TTS_SCRIPT: &str = include_str!("../../../../scripts/infer_sano_tts.py");

/// Write the embedded inference script into the cache on first use. It is
/// rewritten when the embedded copy changes, so an upgrade does not keep an
/// old script around.
fn ensure_tts_script() -> Result<std::path::PathBuf, TtsError> {
    let dir = ravenbot_core::cache_dir().join("tts");
    std::fs::create_dir_all(&dir)
        .map_err(|error| TtsError::Failed(format!("Cannot create TTS cache dir: {error}")))?;
    let path = dir.join("infer_sano_tts.py");
    let current = std::fs::read_to_string(&path).ok();
    if current.as_deref() != Some(TTS_SCRIPT) {
        std::fs::write(&path, TTS_SCRIPT)
            .map_err(|error| TtsError::Failed(format!("Cannot write TTS script: {error}")))?;
    }
    Ok(path)
}

/// The Piper executable. `PIPER_BIN` wins, then the normal PATH lookup.
fn piper_binary() -> String {
    std::env::var("PIPER_BIN")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "piper".to_string())
}

/// Resolve the Piper voice model.
///
/// `PIPER_VOICE` may be an absolute path or a voice name. A name is looked up
/// in `RAVENBOT_HOME/cache/piper/<name>.onnx` (default `~/RAVENBOT/cache`).
/// The default is the documented English voice.
fn piper_model_path() -> Option<PathBuf> {
    let cache = ravenbot_core::cache_dir().join("piper");
    if let Ok(raw) = std::env::var("PIPER_VOICE") {
        let raw = raw.trim();
        if !raw.is_empty() {
            let direct = PathBuf::from(raw);
            if direct.is_file() {
                return Some(direct);
            }
            let named = cache.join(format!("{raw}.onnx"));
            if named.is_file() {
                return Some(named);
            }
        }
    }
    let default = cache.join(format!("{PIPER_DEFAULT_VOICE}.onnx"));
    default.is_file().then_some(default)
}

/// Is a Piper model present and is the executable runnable?
pub fn piper_available() -> bool {
    if piper_model_path().is_none() {
        return false;
    }
    std::process::Command::new(piper_binary())
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub struct TextToSpeech {
    default_options: VoiceOptions,
    /// Optional OpenAI API key for the multi-voice TTS engine (per-bot voices).
    openai_key: Option<String>,
}

impl TextToSpeech {
    pub fn new() -> Self {
        Self {
            default_options: VoiceOptions::default(),
            openai_key: None,
        }
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
        self.openai_key.clone().or_else(|| {
            std::env::var("OPENAI_API_KEY")
                .ok()
                .filter(|k| !k.trim().is_empty())
        })
    }

    /// Synthesize text to WAV bytes. Tries OpenAI (multi-voice) first when a
    /// key is available, then the HF Inference API, then local Python.
    pub async fn synthesize(&self, text: &str) -> Result<Vec<u8>, TtsError> {
        self.synthesize_with_provider(text)
            .await
            .map(|(audio, _)| audio)
    }

    /// The provider is returned so a paid call can be attributed honestly. A
    /// key being present does not mean the paid path produced this audio.
    pub async fn synthesize_with_provider(
        &self,
        text: &str,
    ) -> Result<(Vec<u8>, &'static str), TtsError> {
        if text.trim().is_empty() {
            return Err(TtsError::Failed("Empty text".to_string()));
        }

        // 1. OpenAI speech (supports distinct voices per bot).
        if self.resolved_openai_key().is_some() {
            if let Ok(audio) = self.synthesize_openai(text).await {
                if !audio.is_empty() {
                    return Ok((audio, "openai"));
                }
            }
        }

        // 2. Hugging Face Inference API (fastest, no local GPU needed).
        if let Ok(audio) = self.synthesize_cloud(text).await {
            if !audio.is_empty() {
                return Ok((audio, "huggingface"));
            }
        }

        // 3. Piper — free local ONNX inference, no Python or GPU required.
        if piper_available() {
            if let Ok(audio) = self.synthesize_piper(text).await {
                if !audio.is_empty() {
                    return Ok((audio, "piper"));
                }
            }
        }

        // 4. Fall back to local Python inference with the downloaded model.
        match self.synthesize_local(text).await {
            Ok(audio) if !audio.is_empty() => Ok((audio, "local")),
            Ok(_) => Err(TtsError::NoEngine),
            Err(e) => Err(e),
        }
    }

    /// Piper CLI inference. The text goes over stdin and Piper writes a WAV.
    async fn synthesize_piper(&self, text: &str) -> Result<Vec<u8>, TtsError> {
        let model = piper_model_path().ok_or(TtsError::NoEngine)?;
        let output_dir = ravenbot_core::cache_dir().join("tts");
        std::fs::create_dir_all(&output_dir)
            .map_err(|e| TtsError::Failed(format!("Cannot create TTS cache dir: {e}")))?;
        let output_path = output_dir.join(format!("piper-{}.wav", uuid::Uuid::new_v4()));
        let length_scale = 1.0 / self.default_options.rate.clamp(0.5, 2.0);

        let mut child = tokio::process::Command::new(piper_binary())
            .arg("--model")
            .arg(&model)
            .arg("--output_file")
            .arg(&output_path)
            .arg("--length_scale")
            .arg(format!("{length_scale:.3}"))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                TtsError::Failed(format!(
                    "Could not start Piper ({}). Install piper-tts or set PIPER_BIN: {e}",
                    piper_binary()
                ))
            })?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(text.as_bytes())
                .await
                .map_err(|e| TtsError::Failed(format!("Could not send text to Piper: {e}")))?;
            stdin
                .shutdown()
                .await
                .map_err(|e| TtsError::Failed(format!("Could not close Piper stdin: {e}")))?;
        }

        let output = child
            .wait_with_output()
            .await
            .map_err(|e| TtsError::Failed(format!("Piper failed: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(TtsError::Failed(format!(
                "Piper exited {}: {}",
                output.status.code().unwrap_or(-1),
                stderr.chars().take(300).collect::<String>()
            )));
        }

        let audio = std::fs::read(&output_path)
            .map_err(|e| TtsError::Failed(format!("Failed to read Piper audio: {e}")))?;
        let _ = std::fs::remove_file(&output_path);
        if audio.len() < 100 {
            return Err(TtsError::Failed("Piper returned empty audio".to_string()));
        }
        Ok(audio)
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
            return Err(TtsError::Failed(format!(
                "Inference API error: {}",
                msg.chars().take(300).collect::<String>()
            )));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| TtsError::Failed(format!("Failed to read audio: {}", e)))?;

        // HF returns audio as a JSON with base64 or raw WAV/FLAC bytes.
        // Try to detect: if it starts with RIFF/WAVE or ID3, it's raw audio.
        let is_raw_audio = bytes.len() > 4
            && (bytes.starts_with(b"RIFF")
                || bytes.starts_with(b"ID3\x03")
                || bytes.starts_with(b"fLaC")
                || bytes.starts_with(b"OggS"));

        if is_raw_audio {
            Ok(bytes.to_vec())
        } else {
            // Try parsing as JSON with a base64 audio field.
            if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                if let Some(b64) = val
                    .get("audio")
                    .and_then(|v| v.as_str())
                    .or_else(|| val.as_str())
                {
                    return general_purpose::STANDARD
                        .decode(b64)
                        .map_err(|e| TtsError::Failed(format!("Bad base64 audio: {}", e)))
                        .map_err(|e| TtsError::Failed(format!("Bad base64 audio: {}", e)));
                }
            }
            // Fallback: assume the bytes ARE the audio (some endpoints return raw).
            if bytes.len() > 100 {
                Ok(bytes.to_vec())
            } else {
                Err(TtsError::Failed(
                    "Inference API returned no usable audio".to_string(),
                ))
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
            model_manager::download_model(SANO_TTS_REPO)
                .await
                .map_err(|e| match e {
                    ModelError::DownloadFailed(msg) => {
                        TtsError::Failed(format!("Model download failed: {}", msg))
                    }
                    ModelError::NotFound(f) => {
                        TtsError::Failed(format!("Model file missing: {}", f))
                    }
                    ModelError::Io(e) => TtsError::Failed(format!("IO error: {}", e)),
                })?
        };

        // The script is embedded, not read from the developer checkout. A
        // bundled app does not have `scripts/`, so the old path made local TTS
        // fail in release builds even when the model and Python were present.
        let script_path = ensure_tts_script()?;

        // Write output to a temp file.
        let output_dir = std::env::temp_dir().join("ravenbot_tts");
        let _ = std::fs::create_dir_all(&output_dir);
        let output_path = output_dir.join(format!("{}.wav", uuid::Uuid::new_v4()));

        // Spawn the Python subprocess.
        let python = std::env::var("PYTHON_BIN").unwrap_or_else(|_| "python3".to_string());
        let output = tokio::process::Command::new(&python)
            .arg(&script_path)
            .arg("--model-dir")
            .arg(&model_dir)
            .arg("--text")
            .arg(text)
            .arg("--output")
            .arg(&output_path)
            .output()
            .await
            .map_err(|e| {
                TtsError::Failed(format!(
                    "Failed to start Python ({}). Is Python 3 installed? Error: {}",
                    python, e
                ))
            })?;

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
            return Err(TtsError::Failed(
                "Python returned empty/too-short audio".to_string(),
            ));
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

#[cfg(test)]
mod tests {
    use super::{piper_available, TextToSpeech, TTS_SCRIPT};

    #[test]
    fn the_tts_script_is_embedded_for_bundled_builds() {
        assert!(TTS_SCRIPT.contains("infer_sano_tts.py"));
        assert!(TTS_SCRIPT.contains("argparse"));
        assert!(TTS_SCRIPT.len() > 1000);
    }

    #[tokio::test]
    async fn piper_synthesizes_a_wav_when_installed() {
        // CI has no Piper voice; the test is a real check on machines where the
        // documented free local engine is installed.
        if !piper_available() {
            return;
        }
        let audio = TextToSpeech::new()
            .synthesize_piper("Hello from RAVENBOT.")
            .await
            .expect("piper should synthesize");
        assert_eq!(&audio[..4], b"RIFF");
        assert!(audio.len() > 1_000);
    }

    #[tokio::test]
    async fn piper_is_used_as_the_free_local_provider() {
        let has_paid_key = std::env::var("OPENAI_API_KEY").is_ok()
            || std::env::var("HUGGINGFACE_INFERENCE_TOKEN").is_ok()
            || std::env::var("HF_TOKEN").is_ok();
        if !piper_available() || has_paid_key {
            return;
        }
        let (audio, provider) = TextToSpeech::new()
            .synthesize_with_provider("Hello from RAVENBOT.")
            .await
            .expect("piper should be selected");
        assert_eq!(provider, "piper");
        assert_eq!(&audio[..4], b"RIFF");
    }
}

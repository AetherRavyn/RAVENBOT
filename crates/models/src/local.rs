//! Fully local model provider — zero external network calls
//!
//! Architecture for on-device inference:
//! - `candle` for pure-Rust models (e.g., Phi-2, TinyLlama)
//! - `llama.cpp` bindings for GGUF models (Llama, Mistral, etc.)
//!
//! User can bundle or auto-download weights; once configured,
//! this provider makes **zero** external requests.

use async_trait::async_trait;
use ravenbot_core::ModelProvider;

use super::{ModelProviderTrait, ModelResponse, Message, ToolDefinition, ModelError};

/// Local inference provider — fully offline
pub struct LocalProvider {
    /// Path to model weights (e.g., `./models/llama-3-8b.gguf`)
    model_path: Option<String>,
    /// Whether a model is loaded and ready
    ready: bool,
}

impl Default for LocalProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalProvider {
    pub fn new() -> Self {
        Self {
            model_path: None,
            ready: false,
        }
    }

    /// Configure with a local model path
    pub fn with_model(mut self, path: impl Into<String>) -> Self {
        self.model_path = Some(path.into());
        self
    }

    /// Check if model weights exist at the configured path
    fn weights_available(&self) -> bool {
        self.model_path
            .as_ref()
            .map(|p| std::path::Path::new(p).exists())
            .unwrap_or(false)
    }
}

#[async_trait]
impl ModelProviderTrait for LocalProvider {
    fn provider_type(&self) -> ModelProvider {
        ModelProvider::Local
    }

    async fn complete(
        &self,
        _messages: &[Message],
        _tools: &[ToolDefinition],
        _temperature: f32,
        _max_tokens: u32,
    ) -> Result<ModelResponse, ModelError> {
        if !self.ready || self.model_path.is_none() {
            return Err(ModelError::Provider(
                "No local model configured. Set a model path via Settings → Local Models, \
                 e.g., download a GGUF file and point Ollama or this provider at it. \
                 See docs/local-models.md for setup.".to_string()
            ));
        }

        // Future: dispatch to candle or llama.cpp
        // let ctx = self.load_context().await?;
        // ctx.generate(messages, tools, temperature, max_tokens).await

        Err(ModelError::Provider(
            "Local inference engine not yet linked. Build with --features candle to enable on-device inference.".to_string()
        ))
    }

    /// Apply a model override (treated as local weights path for this provider)
    fn with_model(mut self: Box<Self>, model_id: String) -> Box<dyn ModelProviderTrait> {
        self.model_path = Some(model_id);
        Box::new(*self)
    }

    async fn health_check(&self) -> Result<bool, ModelError> {
        Ok(self.ready && self.weights_available())
    }
}

//! RAVENBOT model provider layer
//!
//! This crate defines the ModelProvider trait and implements various providers
//! for LLM inference.

use ravenbot_core::ModelProvider;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use std::sync::Arc;

pub mod openrouter;
pub mod anthropic;
pub mod openai;
pub mod ollama;
pub mod local;
pub mod manager;
pub mod streaming;

pub use manager::ProviderManager;
pub use streaming::StreamAccumulator;

/// Callback receiving incremental text deltas during streaming.
/// Called from within the provider's response-parsing loop; must be cheap
/// and non-blocking (it forwards to the UI event channel).
pub type DeltaCallback = Arc<dyn Fn(&str) + Send + Sync>;

/// A no-op delta callback for callers that do not need streaming
pub fn noop_delta_callback() -> DeltaCallback {
    Arc::new(|_| {})
}

/// Errors from model providers
#[derive(Error, Debug)]
pub enum ModelError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Provider error: {0}")]
    Provider(String),
    #[error("Rate limited, retry after {retry_after_secs} seconds")]
    RateLimited { retry_after_secs: u64 },
    #[error("Auth error: {0}")]
    Auth(String),
}

/// A message in the conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Role (system, user, assistant)
    pub role: String,
    /// Message content
    pub content: String,
    /// Inline images (base64) attached to this message (vision models)
    #[serde(default)]
    pub images: Vec<MessageImage>,
}

/// An inline image attached to a message (base64-encoded)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageImage {
    /// Base64-encoded image data (no data-URI prefix)
    pub data: String,
    /// MIME type (e.g. image/png)
    pub mime: String,
}

/// Tool/function definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool name
    pub name: String,
    /// Tool description
    pub description: String,
    /// JSON schema for arguments
    pub parameters: serde_json::Value,
}

/// Tool call from the model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Tool name
    pub name: String,
    /// Arguments
    pub arguments: serde_json::Value,
    /// Call ID
    pub id: String,
}

/// Response from a model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelResponse {
    /// Generated content
    pub content: Option<String>,
    /// Tool calls requested
    pub tool_calls: Vec<ToolCall>,
    /// Usage information
    pub usage: Usage,
    /// Chain-of-thought reasoning (extended thinking), if any
    #[serde(default)]
    pub reasoning: Option<String>,
}

/// Token usage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    /// Input tokens
    pub input_tokens: u64,
    /// Output tokens
    pub output_tokens: u64,
}

impl Usage {
    /// Calculate cost based on model pricing
    pub fn cost(&self, input_cost_per_1m: f64, output_cost_per_1m: f64) -> f64 {
        let input_cost = (self.input_tokens as f64 / 1_000_000.0) * input_cost_per_1m;
        let output_cost = (self.output_tokens as f64 / 1_000_000.0) * output_cost_per_1m;
        input_cost + output_cost
    }
}

/// Trait for model providers
#[async_trait::async_trait]
pub trait ModelProviderTrait: Send + Sync {
    /// Get the provider type
    fn provider_type(&self) -> ModelProvider;

    /// Send a completion request
    async fn complete(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        temperature: f32,
        max_tokens: u32,
    ) -> Result<ModelResponse, ModelError>;

    /// Send a completion request with live token streaming.
    ///
    /// The provider forwards each text delta to `on_delta` as it arrives and
    /// returns the fully assembled response (content, tool calls, usage).
    /// The default implementation falls back to a single non-streaming
    /// `complete` call, emitting the whole content as one delta — providers
    /// without SSE support still work transparently.
    ///
    /// `enable_reasoning` requests extended thinking (visible chain-of-thought)
    /// for providers that support it; unsupported providers ignore it.
    async fn complete_stream(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        temperature: f32,
        max_tokens: u32,
        on_delta: DeltaCallback,
        enable_reasoning: bool,
    ) -> Result<ModelResponse, ModelError> {
        let _ = enable_reasoning;
        let response = self.complete(messages, tools, temperature, max_tokens).await?;
        if let Some(content) = &response.content {
            if !content.is_empty() {
                on_delta(content);
            }
        }
        Ok(response)
    }

    /// Check if the provider is available
    async fn health_check(&self) -> Result<bool, ModelError>;

    /// Apply a user-configured model id override (consumes the boxed provider)
    fn with_model(self: Box<Self>, model_id: String) -> Box<dyn ModelProviderTrait>;
}

/// Create a model provider from configuration
pub fn create_provider(
    provider: ModelProvider,
    api_key: Option<String>,
    base_url: Option<String>,
) -> Box<dyn ModelProviderTrait> {
    match provider {
        ModelProvider::OpenRouter => Box::new(openrouter::OpenRouterProvider::new(api_key)),
        ModelProvider::Anthropic => Box::new(anthropic::AnthropicProvider::new(api_key)),
        ModelProvider::OpenAI => Box::new(openai::OpenAIProvider::new(api_key)),
        ModelProvider::Ollama => Box::new(ollama::OllamaProvider::new(base_url)),
        ModelProvider::Local => Box::new(local::LocalProvider::new()),
    }
}

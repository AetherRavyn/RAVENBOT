//! RAVENBOT model provider layer
//!
//! This crate defines the ModelProvider trait and implements various providers
//! for LLM inference.

use ravenbot_core::ModelProvider;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

pub mod anthropic;
pub mod commandcode;
pub mod discovery;
pub mod local;
pub mod manager;
pub mod mimo;
pub mod ollama;
pub mod openai;
pub mod openai_compat;
pub mod opencode_go;
pub mod openrouter;
pub mod streaming;

pub use discovery::{DiscoveredModel, ModelDiscovery};
pub use manager::ProviderManager;
pub use streaming::StreamAccumulator;

/// Seconds from an environment variable, with a positive-value floor.
fn env_secs(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

/// HTTP client for model providers.
///
/// A request total-timeout would kill long generations and SSE streams, so
/// this uses a connect timeout and an idle read timeout instead: the stream can
/// run for as long as it keeps producing data, but a dead socket cannot pin an
/// agent forever. Both limits are environment-tunable for slow networks.
pub fn http_client() -> reqwest::Client {
    http_client_with_timeouts(
        Duration::from_secs(env_secs("RAVENBOT_MODEL_CONNECT_TIMEOUT_SECS", 15)),
        Duration::from_secs(env_secs("RAVENBOT_MODEL_READ_TIMEOUT_SECS", 300)),
    )
}

pub fn http_client_with_timeouts(connect: Duration, read: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("ravenbot/0.2.0 (https://github.com/AetherRavyn/RAVENBOT)")
        .connect_timeout(connect)
        .read_timeout(read)
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            match ravenbot_core::net::validate_http_url(attempt.url().as_str()) {
                Ok(_) => attempt.follow(),
                Err(error) => attempt.error(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    error,
                )),
            }
        }))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

/// One piece of a streamed model response.
///
/// Reasoning is a *separate* variant rather than being prefixed into the text,
/// for three reasons that all bite in practice:
///
///  1. **It must not be wiped.** The runtime clears the streamed text buffer at
///     every tool round, so reasoning smuggled through the text channel
///     disappears the moment an agent uses a tool — which is precisely when
///     reasoning is most worth reading.
///  2. **It must not be read as answer text.** A `<think>` marker left open
///     mid-stream puts a model's private notes into the paragraph a user is
///     reading, and nothing strips it reliably.
///  3. **Order has to survive.** One tagged channel keeps reasoning interleaved
///     with text in the order the model produced it, so a live view can show
///     "thought, then said, then thought again" instead of two streams whose
///     relative timing was thrown away at the callback boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamChunk<'a> {
    /// Ordinary assistant output — what the user is meant to read.
    Text(&'a str),
    /// Extended-thinking / reasoning tokens. Never shown as answer text.
    Reasoning(&'a str),
}

impl<'a> StreamChunk<'a> {
    /// The payload, whichever variant this is.
    pub fn as_str(&self) -> &'a str {
        match self {
            StreamChunk::Text(s) | StreamChunk::Reasoning(s) => s,
        }
    }

    /// Whether this is answer text, for callers that only care about that.
    pub fn is_text(&self) -> bool {
        matches!(self, StreamChunk::Text(_))
    }
}

/// Callback receiving incremental chunks during streaming.
/// Called from within the provider's response-parsing loop; must be cheap
/// and non-blocking (it forwards to the UI event channel).
pub type DeltaCallback = Arc<dyn Fn(StreamChunk<'_>) + Send + Sync>;

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
    /// The HTTP stream ended without the provider's terminal signal. The
    /// partial text is preserved so the runtime can persist it honestly.
    #[error("model stream ended before completion")]
    IncompleteStream { partial: String },
}

/// A message in the conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Role (system, user, assistant, tool)
    pub role: String,
    /// Message content
    pub content: String,
    /// Inline images (base64) attached to this message (vision models)
    #[serde(default)]
    pub images: Vec<MessageImage>,
    /// Native assistant tool calls for this turn (empty for non-assistant
    /// messages). Round-tripping these with their ids is what lets models
    /// continue a tool loop correctly instead of seeing flattened text.
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    /// For `role == "tool"`: the id of the tool call this message answers.
    #[serde(default)]
    pub tool_call_id: Option<String>,
    /// Optional tool/function name (legacy OpenAI `name`, Anthropic bookkeeping)
    #[serde(default)]
    pub name: Option<String>,
}

impl Message {
    /// Plain text message with a role.
    pub fn text(role: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
            images: Vec::new(),
            tool_calls: Vec::new(),
            tool_call_id: None,
            name: None,
        }
    }

    /// Attach inline images (vision).
    pub fn with_images(mut self, images: Vec<MessageImage>) -> Self {
        self.images = images;
        self
    }

    /// Assistant message carrying native tool calls.
    pub fn assistant_tool_calls(content: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.into(),
            images: Vec::new(),
            tool_calls,
            tool_call_id: None,
            name: None,
        }
    }

    /// Tool result message answering a specific tool call.
    pub fn tool_result(
        tool_call_id: impl Into<String>,
        name: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            role: "tool".to_string(),
            content: content.into(),
            images: Vec::new(),
            tool_calls: Vec::new(),
            tool_call_id: Some(tool_call_id.into()),
            name: Some(name.into()),
        }
    }

    /// Whether this is an assistant turn that requested tools.
    pub fn has_tool_calls(&self) -> bool {
        self.role == "assistant" && !self.tool_calls.is_empty()
    }

    /// OpenAI-wire content value: a plain string, or a text + image_url parts
    /// array when the message carries inline images (vision models).
    pub fn openai_content(&self) -> serde_json::Value {
        if self.images.is_empty() {
            return serde_json::json!(self.content);
        }
        let mut parts = vec![serde_json::json!({"type": "text", "text": self.content})];
        for img in &self.images {
            parts.push(serde_json::json!({
                "type": "image_url",
                "image_url": { "url": format!("data:{};base64,{}", img.mime, img.data) }
            }));
        }
        serde_json::Value::Array(parts)
    }

    /// OpenAI-wire assistant tool calls (`arguments` serialized as a string).
    pub fn openai_tool_calls(&self) -> Option<Vec<WireToolCall>> {
        if self.tool_calls.is_empty() {
            return None;
        }
        Some(
            self.tool_calls
                .iter()
                .map(|tc| WireToolCall {
                    id: tc.id.clone(),
                    call_type: "function".to_string(),
                    function: WireFunction {
                        name: tc.name.clone(),
                        arguments: serde_json::to_string(&tc.arguments)
                            .unwrap_or_else(|_| "{}".to_string()),
                    },
                })
                .collect(),
        )
    }
}

/// OpenAI-wire tool call (shared across openai-wire providers).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: WireFunction,
}

/// OpenAI-wire tool call function payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireFunction {
    pub name: String,
    pub arguments: String,
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
        let response = self
            .complete(messages, tools, temperature, max_tokens)
            .await?;
        if let Some(content) = &response.content {
            if !content.is_empty() {
                on_delta(StreamChunk::Text(content));
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
        ModelProvider::CommandCode => Box::new(commandcode::CommandCodeProvider::new(api_key)),
        ModelProvider::DeepSeek => Box::new(openai_compat::deepseek_provider(api_key)),
        ModelProvider::Groq => Box::new(openai_compat::groq_provider(api_key)),
        ModelProvider::Gemini => Box::new(openai_compat::gemini_provider(api_key)),
        ModelProvider::Mistral => Box::new(openai_compat::mistral_provider(api_key)),
        ModelProvider::Together => Box::new(openai_compat::together_provider(api_key)),
        ModelProvider::Perplexity => Box::new(openai_compat::perplexity_provider(api_key)),
        ModelProvider::Cohere => Box::new(openai_compat::cohere_provider(api_key)),
        ModelProvider::MiMo => Box::new(mimo::MiMoProvider::new(api_key)),
        ModelProvider::OpenCode => Box::new(openai_compat::opencode_provider(api_key)),
        ModelProvider::OpenCodeGo => Box::new(opencode_go::OpenCodeGoProvider::new(api_key)),
        ModelProvider::Cline => Box::new(openai_compat::cline_provider(api_key)),
        ModelProvider::XAI => Box::new(openai_compat::xai_provider(api_key)),
        ModelProvider::TokenRouter => Box::new(openai_compat::tokenrouter_provider(api_key)),
        // Custom providers are fully constructed by ProviderManager (it knows
        // the kind); this arm is a safe fallback for direct factory callers.
        ModelProvider::Custom => Box::new(openai_compat::custom_provider(
            api_key,
            base_url.unwrap_or_default(),
            "",
        )),
    }
}

#[cfg(test)]
mod timeout_tests {
    use super::*;
    use std::time::Instant;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn a_stalled_provider_connection_hits_the_read_timeout() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test listener");
        let address = listener.local_addr().expect("listener address");
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut request = [0u8; 2048];
            let _ = socket.read(&mut request).await;
            // Accept the request and never answer. This is the black-hole
            // provider that used to pin an agent indefinitely.
            tokio::time::sleep(Duration::from_secs(10)).await;
        });

        let client = http_client_with_timeouts(Duration::from_secs(2), Duration::from_millis(300));
        let started = Instant::now();
        let error = client
            .get(format!("http://{address}/"))
            .send()
            .await
            .expect_err("a stalled response must fail");
        assert!(error.is_timeout(), "expected a timeout, got {error:?}");
        assert!(started.elapsed() < Duration::from_secs(5));
        server.abort();
    }

    #[tokio::test]
    async fn a_redirect_to_cloud_metadata_is_blocked() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test listener");
        let address = listener.local_addr().expect("listener address");
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut request = [0u8; 2048];
            let _ = socket.read(&mut request).await;
            let response = "HTTP/1.1 302 Found\r\nLocation: http://169.254.169.254/latest/meta-data\r\nContent-Length: 0\r\n\r\n";
            let _ = socket.write_all(response.as_bytes()).await;
        });

        let client = http_client_with_timeouts(Duration::from_secs(2), Duration::from_secs(2));
        let error = client
            .get(format!("http://{address}/"))
            .send()
            .await
            .expect_err("a metadata redirect must be blocked");
        assert!(
            error.to_string().contains("metadata") || error.is_redirect(),
            "unexpected redirect error: {error:?}"
        );
        server.abort();
    }
}

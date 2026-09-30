use serde::{Deserialize, Serialize};
/// Supported model providers
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ModelProvider {
    /// OpenRouter
    OpenRouter,
    /// Anthropic
    Anthropic,
    /// OpenAI
    OpenAI,
    /// Ollama (local)
    Ollama,
    /// Fully local inference (candle/llama.cpp)
    Local,
}

impl ModelProvider {
    /// Display name for the provider
    pub fn display_name(&self) -> &str {
        match self {
            Self::OpenRouter => "OpenRouter",
            Self::Anthropic => "Anthropic",
            Self::OpenAI => "OpenAI",
            Self::Ollama => "Ollama",
            Self::Local => "Local",
        }
    }

    /// Whether this provider requires network access
    pub fn requires_network(&self) -> bool {
        match self {
            Self::OpenRouter | Self::Anthropic | Self::OpenAI => true,
            Self::Ollama | Self::Local => false,
        }
    }
}

/// A model configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Provider
    pub provider: ModelProvider,
    /// Model identifier (e.g., "anthropic/claude-3-5-sonnet")
    pub model_id: String,
    /// Display name
    pub display_name: String,
    /// Max context window size
    pub context_window: u32,
    /// Max output tokens
    pub max_output_tokens: u32,
    /// Cost per 1M input tokens (in dollars)
    pub input_cost_per_1m: f64,
    /// Cost per 1M output tokens (in dollars)
    pub output_cost_per_1m: f64,
    /// Whether this model supports vision
    pub supports_vision: bool,
    /// Whether this model supports tool use
    pub supports_tools: bool,
}

/// Pre-configured models
pub fn builtin_models() -> Vec<ModelConfig> {
    vec![
        ModelConfig {
            provider: ModelProvider::Anthropic,
            model_id: "claude-3-5-sonnet-20241022".to_string(),
            display_name: "Claude 3.5 Sonnet".to_string(),
            context_window: 200000,
            max_output_tokens: 8192,
            input_cost_per_1m: 3.0,
            output_cost_per_1m: 15.0,
            supports_vision: true,
            supports_tools: true,
        },
        ModelConfig {
            provider: ModelProvider::OpenAI,
            model_id: "gpt-4o".to_string(),
            display_name: "GPT-4o".to_string(),
            context_window: 128000,
            max_output_tokens: 16384,
            input_cost_per_1m: 2.5,
            output_cost_per_1m: 10.0,
            supports_vision: true,
            supports_tools: true,
        },
        ModelConfig {
            provider: ModelProvider::Ollama,
            model_id: "llama3.1:8b".to_string(),
            display_name: "Llama 3.1 8B (Local)".to_string(),
            context_window: 8192,
            max_output_tokens: 4096,
            input_cost_per_1m: 0.0,
            output_cost_per_1m: 0.0,
            supports_vision: false,
            supports_tools: true,
        },
    ]
}

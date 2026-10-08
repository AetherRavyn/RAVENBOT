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
    /// Command Code (OpenAI/Anthropic compatible proxy)
    CommandCode,
    /// DeepSeek
    DeepSeek,
    /// Groq (fast inference)
    Groq,
    /// Google Gemini
    Gemini,
    /// Mistral AI
    Mistral,
    /// Together AI
    Together,
    /// Perplexity
    Perplexity,
    /// Cohere
    Cohere,
    /// Xiaomi MiMo
    MiMo,
    /// OpenCode
    OpenCode,
    /// Cline
    Cline,
    /// xAI Grok
    XAI,
    /// TokenRouter (unified multi-model gateway)
    TokenRouter,
    /// User-defined provider (OpenAI-compatible, Anthropic, or Ollama endpoint)
    Custom,
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
            Self::CommandCode => "Command Code",
            Self::DeepSeek => "DeepSeek",
            Self::Groq => "Groq",
            Self::Gemini => "Gemini",
            Self::Mistral => "Mistral",
            Self::Together => "Together",
            Self::Perplexity => "Perplexity",
            Self::Cohere => "Cohere",
            Self::MiMo => "MiMo",
            Self::OpenCode => "OpenCode",
            Self::Cline => "Cline",
            Self::XAI => "xAI Grok",
            Self::TokenRouter => "TokenRouter",
            Self::Custom => "Custom",
        }
    }

    /// Whether this provider requires network access
    pub fn requires_network(&self) -> bool {
        match self {
            Self::Ollama | Self::Local => false,
            _ => true,
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
        // ── Anthropic ───────────────────────────────────────────────────
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
            provider: ModelProvider::Anthropic,
            model_id: "claude-3-opus-20240229".to_string(),
            display_name: "Claude 3 Opus".to_string(),
            context_window: 200000,
            max_output_tokens: 4096,
            input_cost_per_1m: 15.0,
            output_cost_per_1m: 75.0,
            supports_vision: true,
            supports_tools: true,
        },
        // ── OpenAI ──────────────────────────────────────────────────────
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
            provider: ModelProvider::OpenAI,
            model_id: "gpt-4o-mini".to_string(),
            display_name: "GPT-4o Mini".to_string(),
            context_window: 128000,
            max_output_tokens: 16384,
            input_cost_per_1m: 0.15,
            output_cost_per_1m: 0.6,
            supports_vision: true,
            supports_tools: true,
        },
        ModelConfig {
            provider: ModelProvider::OpenAI,
            model_id: "o1-preview".to_string(),
            display_name: "o1 Preview".to_string(),
            context_window: 128000,
            max_output_tokens: 32768,
            input_cost_per_1m: 15.0,
            output_cost_per_1m: 60.0,
            supports_vision: false,
            supports_tools: true,
        },
        // ── Ollama ──────────────────────────────────────────────────────
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
        // ── Command Code ─────────────────────────────────────────────────
        ModelConfig {
            provider: ModelProvider::CommandCode,
            model_id: "deepseek/deepseek-v4-flash".to_string(),
            display_name: "DeepSeek V4 Flash (CC)".to_string(),
            context_window: 128000,
            max_output_tokens: 8192,
            input_cost_per_1m: 0.07,
            output_cost_per_1m: 0.28,
            supports_vision: false,
            supports_tools: true,
        },
        ModelConfig {
            provider: ModelProvider::CommandCode,
            model_id: "claude-sonnet-4-6".to_string(),
            display_name: "Claude Sonnet 4 (CC)".to_string(),
            context_window: 200000,
            max_output_tokens: 8192,
            input_cost_per_1m: 3.0,
            output_cost_per_1m: 15.0,
            supports_vision: true,
            supports_tools: true,
        },
        ModelConfig {
            provider: ModelProvider::CommandCode,
            model_id: "longcat-2.0:free".to_string(),
            display_name: "LongCat 2.0 Free (CC)".to_string(),
            context_window: 128000,
            max_output_tokens: 8192,
            input_cost_per_1m: 0.0,
            output_cost_per_1m: 0.0,
            supports_vision: false,
            supports_tools: true,
        },
        // ── DeepSeek ────────────────────────────────────────────────────
        ModelConfig {
            provider: ModelProvider::DeepSeek,
            model_id: "deepseek-chat".to_string(),
            display_name: "DeepSeek V3".to_string(),
            context_window: 64000,
            max_output_tokens: 8192,
            input_cost_per_1m: 0.07,
            output_cost_per_1m: 0.28,
            supports_vision: false,
            supports_tools: true,
        },
        ModelConfig {
            provider: ModelProvider::DeepSeek,
            model_id: "deepseek-reasoner".to_string(),
            display_name: "DeepSeek R1".to_string(),
            context_window: 64000,
            max_output_tokens: 8192,
            input_cost_per_1m: 0.14,
            output_cost_per_1m: 0.55,
            supports_vision: false,
            supports_tools: true,
        },
        // ── Groq ────────────────────────────────────────────────────────
        ModelConfig {
            provider: ModelProvider::Groq,
            model_id: "llama-3.3-70b-versatile".to_string(),
            display_name: "Llama 3.3 70B (Groq)".to_string(),
            context_window: 128000,
            max_output_tokens: 32768,
            input_cost_per_1m: 0.0,
            output_cost_per_1m: 0.0,
            supports_vision: false,
            supports_tools: true,
        },
        ModelConfig {
            provider: ModelProvider::Groq,
            model_id: "mixtral-8x7b-32768".to_string(),
            display_name: "Mixtral 8x7B (Groq)".to_string(),
            context_window: 32768,
            max_output_tokens: 32768,
            input_cost_per_1m: 0.0,
            output_cost_per_1m: 0.0,
            supports_vision: false,
            supports_tools: true,
        },
        // ── Gemini ──────────────────────────────────────────────────────
        ModelConfig {
            provider: ModelProvider::Gemini,
            model_id: "gemini-2.0-flash".to_string(),
            display_name: "Gemini 2.0 Flash".to_string(),
            context_window: 1000000,
            max_output_tokens: 8192,
            input_cost_per_1m: 0.1,
            output_cost_per_1m: 0.4,
            supports_vision: true,
            supports_tools: true,
        },
        ModelConfig {
            provider: ModelProvider::Gemini,
            model_id: "gemini-2.5-pro".to_string(),
            display_name: "Gemini 2.5 Pro".to_string(),
            context_window: 1000000,
            max_output_tokens: 8192,
            input_cost_per_1m: 1.25,
            output_cost_per_1m: 10.0,
            supports_vision: true,
            supports_tools: true,
        },
        // ── Mistral ─────────────────────────────────────────────────────
        ModelConfig {
            provider: ModelProvider::Mistral,
            model_id: "mistral-large-latest".to_string(),
            display_name: "Mistral Large".to_string(),
            context_window: 128000,
            max_output_tokens: 8192,
            input_cost_per_1m: 2.0,
            output_cost_per_1m: 6.0,
            supports_vision: false,
            supports_tools: true,
        },
        ModelConfig {
            provider: ModelProvider::Mistral,
            model_id: "mistral-small-latest".to_string(),
            display_name: "Mistral Small".to_string(),
            context_window: 32000,
            max_output_tokens: 8192,
            input_cost_per_1m: 0.2,
            output_cost_per_1m: 0.6,
            supports_vision: false,
            supports_tools: true,
        },
        // ── Together ────────────────────────────────────────────────────
        ModelConfig {
            provider: ModelProvider::Together,
            model_id: "meta-llama/Llama-3.3-70B-Instruct-Turbo".to_string(),
            display_name: "Llama 3.3 70B (Together)".to_string(),
            context_window: 128000,
            max_output_tokens: 8192,
            input_cost_per_1m: 0.88,
            output_cost_per_1m: 0.88,
            supports_vision: false,
            supports_tools: true,
        },
        ModelConfig {
            provider: ModelProvider::Together,
            model_id: "deepseek-ai/DeepSeek-R1".to_string(),
            display_name: "DeepSeek R1 (Together)".to_string(),
            context_window: 128000,
            max_output_tokens: 8192,
            input_cost_per_1m: 1.5,
            output_cost_per_1m: 1.5,
            supports_vision: false,
            supports_tools: true,
        },
        // ── Perplexity ──────────────────────────────────────────────────
        ModelConfig {
            provider: ModelProvider::Perplexity,
            model_id: "llama-3.1-sonar-large-128k-online".to_string(),
            display_name: "Sonar Large (Perplexity)".to_string(),
            context_window: 128000,
            max_output_tokens: 8192,
            input_cost_per_1m: 1.0,
            output_cost_per_1m: 1.0,
            supports_vision: false,
            supports_tools: true,
        },
        // ── Cohere ──────────────────────────────────────────────────────
        ModelConfig {
            provider: ModelProvider::Cohere,
            model_id: "command-r-plus".to_string(),
            display_name: "Command R+".to_string(),
            context_window: 128000,
            max_output_tokens: 4096,
            input_cost_per_1m: 2.5,
            output_cost_per_1m: 10.0,
            supports_vision: false,
            supports_tools: true,
        },
        ModelConfig {
            provider: ModelProvider::Cohere,
            model_id: "command-r".to_string(),
            display_name: "Command R".to_string(),
            context_window: 128000,
            max_output_tokens: 4096,
            input_cost_per_1m: 0.15,
            output_cost_per_1m: 0.6,
            supports_vision: false,
            supports_tools: true,
        },
        // ── MiMo ────────────────────────────────────────────────────────
        ModelConfig {
            provider: ModelProvider::MiMo,
            model_id: "mimo-v2.5".to_string(),
            display_name: "MiMo V2.5".to_string(),
            context_window: 128000,
            max_output_tokens: 8192,
            input_cost_per_1m: 0.0,
            output_cost_per_1m: 0.0,
            supports_vision: false,
            supports_tools: true,
        },
        ModelConfig {
            provider: ModelProvider::MiMo,
            model_id: "mimo-v2.5-pro".to_string(),
            display_name: "MiMo V2.5 Pro".to_string(),
            context_window: 128000,
            max_output_tokens: 8192,
            input_cost_per_1m: 0.5,
            output_cost_per_1m: 1.0,
            supports_vision: false,
            supports_tools: true,
        },
        // ── OpenCode ────────────────────────────────────────────────────
        ModelConfig {
            provider: ModelProvider::OpenCode,
            model_id: "claude-sonnet-4-5".to_string(),
            display_name: "OpenCode Claude Sonnet 4.5".to_string(),
            context_window: 200000,
            max_output_tokens: 8192,
            input_cost_per_1m: 3.0,
            output_cost_per_1m: 15.0,
            supports_vision: true,
            supports_tools: true,
        },
        // ── Cline ───────────────────────────────────────────────────────
        ModelConfig {
            provider: ModelProvider::Cline,
            model_id: "claude-sonnet-4-5".to_string(),
            display_name: "Cline Claude Sonnet 4.5".to_string(),
            context_window: 200000,
            max_output_tokens: 8192,
            input_cost_per_1m: 3.0,
            output_cost_per_1m: 15.0,
            supports_vision: true,
            supports_tools: true,
        },
        // ── xAI Grok ────────────────────────────────────────────────────
        ModelConfig {
            provider: ModelProvider::XAI,
            model_id: "grok-2-latest".to_string(),
            display_name: "Grok 2".to_string(),
            context_window: 131072,
            max_output_tokens: 8192,
            input_cost_per_1m: 2.0,
            output_cost_per_1m: 10.0,
            supports_vision: true,
            supports_tools: true,
        },
    ]
}

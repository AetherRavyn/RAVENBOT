//! OpenCode Go: one subscription key, two wire protocols.
//!
//! The Go endpoint serves most models over the OpenAI Chat Completions wire
//! format, but Claude models are only reachable through the Anthropic Messages
//! endpoint. OpenCode publishes that split as `provider.npm` in the models.dev
//! registry; the catalog uses the same metadata to filter the model list, and
//! this provider routes each call to the matching protocol.
//!
//! GPT/Grok/Muse (served over `/responses`) and Gemini (`/google`) are not
//! implemented here yet. They are filtered out of the catalog rather than
//! offered and failing with `ModelProtocolUnsupported`.
//!
//! The model list itself is **not** hardcoded: discovery reads it from
//! `https://opencode.ai/inference/go/v1/models` and enriches it from
//! `https://models.dev/api.json`, so a model added to Go appears in RAVENBOT
//! the next time discovery runs.

use async_trait::async_trait;
use ravenbot_core::ModelProvider;

use crate::anthropic::AnthropicProvider;
use crate::openai_compat;
use crate::{
    DeltaCallback, Message, ModelError, ModelProviderTrait, ModelResponse, ToolDefinition,
};

pub const OPENAI_BASE: &str = "https://opencode.ai/inference/go/openai/v1";
pub const ANTHROPIC_BASE: &str = "https://opencode.ai/inference/go/anthropic/v1";
pub const DEFAULT_MODEL: &str = "deepseek-v4.1-flash";

/// Whether this model id is served by the Anthropic Messages endpoint.
///
/// Claude is the one family Go exposes only through Anthropic. The check is
/// deliberately narrow: Qwen/MiniMax are also advertised with the Anthropic
/// SDK by OpenCode but accept the OpenAI wire format too, and routing them the
/// other way would waste a working path.
pub fn is_anthropic_model(model_id: &str) -> bool {
    model_id.trim().to_ascii_lowercase().starts_with("claude")
}

pub struct OpenCodeGoProvider {
    api_key: Option<String>,
    model_id: String,
}

impl OpenCodeGoProvider {
    pub fn new(api_key: Option<String>) -> Self {
        Self {
            api_key,
            model_id: DEFAULT_MODEL.to_string(),
        }
    }

    pub fn with_model_id(mut self, model_id: impl Into<String>) -> Self {
        self.model_id = model_id.into();
        self
    }

    fn openai(&self) -> Box<dyn ModelProviderTrait> {
        Box::new(openai_compat::opencode_go_provider(self.api_key.clone()))
            .with_model(self.model_id.clone())
    }

    fn anthropic(&self) -> Box<dyn ModelProviderTrait> {
        let provider = AnthropicProvider::new(self.api_key.clone())
            .with_base_url(ANTHROPIC_BASE)
            .with_headers(openai_compat::opencode_session_headers());
        Box::new(provider).with_model(self.model_id.clone())
    }
}

#[async_trait]
impl ModelProviderTrait for OpenCodeGoProvider {
    fn provider_type(&self) -> ModelProvider {
        ModelProvider::OpenCodeGo
    }

    async fn complete(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        temperature: f32,
        max_tokens: u32,
    ) -> Result<ModelResponse, ModelError> {
        if is_anthropic_model(&self.model_id) {
            self.anthropic()
                .complete(messages, tools, temperature, max_tokens)
                .await
        } else {
            self.openai()
                .complete(messages, tools, temperature, max_tokens)
                .await
        }
    }

    async fn complete_stream(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        temperature: f32,
        max_tokens: u32,
        on_delta: DeltaCallback,
        enable_reasoning: bool,
    ) -> Result<ModelResponse, ModelError> {
        if is_anthropic_model(&self.model_id) {
            self.anthropic()
                .complete_stream(
                    messages,
                    tools,
                    temperature,
                    max_tokens,
                    on_delta,
                    enable_reasoning,
                )
                .await
        } else {
            self.openai()
                .complete_stream(
                    messages,
                    tools,
                    temperature,
                    max_tokens,
                    on_delta,
                    enable_reasoning,
                )
                .await
        }
    }

    fn with_model(mut self: Box<Self>, model_id: String) -> Box<dyn ModelProviderTrait> {
        self.model_id = model_id;
        self
    }

    async fn health_check(&self) -> Result<bool, ModelError> {
        self.openai().health_check().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_claude_models_route_to_anthropic() {
        assert!(is_anthropic_model("claude-haiku-5-5"));
        assert!(is_anthropic_model("Claude-Haiku-5-5"));
        // Advertised with the Anthropic SDK by OpenCode, but also accepts the
        // OpenAI wire format — must stay on the OpenAI path.
        assert!(!is_anthropic_model("qwen3.8-flash"));
        assert!(!is_anthropic_model("minimax-m3"));
        assert!(!is_anthropic_model("deepseek-v4.1-flash"));
    }

    #[test]
    fn one_key_serves_both_protocols() {
        let provider = OpenCodeGoProvider::new(Some("k".into()));
        assert_eq!(provider.provider_type(), ModelProvider::OpenCodeGo);
        let routed = provider.with_model_id("claude-haiku-5-5");
        assert_eq!(routed.model_id, "claude-haiku-5-5");
        assert_eq!(
            OpenCodeGoProvider::new(None).model_id,
            "deepseek-v4.1-flash"
        );
    }
}

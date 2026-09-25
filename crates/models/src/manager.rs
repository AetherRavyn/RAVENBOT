//! Model provider manager
//!
//! Manages API keys and creates model providers on demand.

use std::collections::HashMap;
use ravenbot_core::ModelProvider;

use super::{ModelProviderTrait, ModelError, create_provider};
use super::anthropic::AnthropicProvider;
use super::ollama::OllamaProvider;
use super::openai_compat;

/// A user-defined provider (migration 018 `custom_providers`).
#[derive(Debug, Clone)]
pub struct CustomProviderSpec {
    pub id: String,
    pub display_name: String,
    /// "openai" | "anthropic" | "ollama"
    pub kind: String,
    pub base_url: String,
    pub default_model: String,
    pub supports_tools: bool,
}

pub struct ProviderManager {
    api_keys: HashMap<String, String>,
    base_urls: HashMap<String, String>,
    custom: HashMap<String, CustomProviderSpec>,
}

impl ProviderManager {
    pub fn new() -> Self {
        let mut api_keys = HashMap::new();
        let mut base_urls = HashMap::new();

        // Check environment variables for default keys
        for (env_var, key_name) in &[
            ("OPENROUTER_API_KEY", "openrouter"),
            ("ANTHROPIC_API_KEY", "anthropic"),
            ("OPENAI_API_KEY", "openai"),
            ("COMMANDCODE_API_KEY", "commandcode"),
            ("DEEPSEEK_API_KEY", "deepseek"),
            ("GROQ_API_KEY", "groq"),
            ("GEMINI_API_KEY", "gemini"),
            ("MISTRAL_API_KEY", "mistral"),
            ("TOGETHER_API_KEY", "together"),
            ("PERPLEXITY_API_KEY", "perplexity"),
            ("COHERE_API_KEY", "cohere"),
            ("MIMO_API_KEY", "mimo"),
            ("OPENCODE_API_KEY", "opencode"),
            ("CLINE_API_KEY", "cline"),
            ("XAI_API_KEY", "xai"),
            ("GROK_API_KEY", "xai"),
            ("TOKENROUTER_API_KEY", "tokenrouter"),
            ("TOKEN_ROUTER_API_KEY", "tokenrouter"),
            ("CLAUDE_API_KEY", "anthropic"),
        ] {
            if let Ok(k) = std::env::var(env_var) {
                if !k.trim().is_empty() {
                    api_keys.insert(key_name.to_string(), k.trim().to_string());
                }
            }
        }

        // Ollama base URL
        if let Ok(u) = std::env::var("OLLAMA_HOST") {
            if !u.trim().is_empty() {
                base_urls.insert("ollama".to_string(), u.trim().to_string());
            }
        } else if let Ok(u) = std::env::var("OLLAMA_URL") {
            if !u.trim().is_empty() {
                base_urls.insert("ollama".to_string(), u.trim().to_string());
            }
        }

        Self {
            api_keys,
            base_urls,
            custom: HashMap::new(),
        }
    }

    /// Set an API key for a provider
    pub fn set_api_key(&mut self, provider: &str, key: String) {
        self.api_keys.insert(provider.to_string(), key);
    }

    /// Remove an API key (e.g. when the user clears it in Settings)
    pub fn remove_api_key(&mut self, provider: &str) {
        self.api_keys.remove(provider);
    }

    /// Get an API key for a provider
    pub fn get_api_key(&self, provider: &str) -> Option<&str> {
        self.api_keys.get(provider).map(|s| s.as_str())
    }

    /// Set a base URL for a provider (useful for Ollama)
    pub fn set_base_url(&mut self, provider: &str, url: String) {
        self.base_urls.insert(provider.to_string(), url);
    }

    /// Get a base URL for a provider
    pub fn get_base_url(&self, provider: &str) -> Option<&str> {
        self.base_urls.get(provider).map(|s| s.as_str())
    }

    /// Register (or replace) a user-defined provider
    pub fn register_custom(&mut self, spec: CustomProviderSpec) {
        self.custom.insert(spec.id.to_lowercase(), spec);
    }

    /// Remove a user-defined provider registration
    pub fn remove_custom(&mut self, id: &str) {
        self.custom.remove(&id.to_lowercase());
    }

    /// Look up a user-defined provider spec by id
    pub fn custom_spec(&self, id: &str) -> Option<&CustomProviderSpec> {
        self.custom.get(&id.to_lowercase())
    }

    /// All registered custom provider ids
    pub fn custom_provider_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.custom.keys().cloned().collect();
        ids.sort();
        ids
    }

    /// Create a model provider
    pub fn create_provider(&self, provider: ModelProvider) -> Box<dyn ModelProviderTrait> {
        self.create_provider_with_model(provider, None)
    }

    /// Create a model provider honoring a user-configured model id.
    /// The model id from `BotConfig.model_id` was previously ignored;
    /// this is the plumbing that applies it.
    pub fn create_provider_with_model(
        &self,
        provider: ModelProvider,
        model_id: Option<&str>,
    ) -> Box<dyn ModelProviderTrait> {
        let provider_str = match &provider {
            ModelProvider::OpenRouter => "openrouter",
            ModelProvider::Anthropic => "anthropic",
            ModelProvider::OpenAI => "openai",
            ModelProvider::Ollama => "ollama",
            ModelProvider::Local => "local",
            ModelProvider::CommandCode => "commandcode",
            ModelProvider::DeepSeek => "deepseek",
            ModelProvider::Groq => "groq",
            ModelProvider::Gemini => "gemini",
            ModelProvider::Mistral => "mistral",
            ModelProvider::Together => "together",
            ModelProvider::Perplexity => "perplexity",
            ModelProvider::Cohere => "cohere",
            ModelProvider::MiMo => "mimo",
            ModelProvider::OpenCode => "opencode",
            ModelProvider::Cline => "cline",
            ModelProvider::XAI => "xai",
            ModelProvider::TokenRouter => "tokenrouter",
            ModelProvider::Custom => "custom",
        };

        let api_key = self.api_keys.get(provider_str)
            .or_else(|| if provider_str == "anthropic" { self.api_keys.get("claude") } else { None })
            .or_else(|| if provider_str == "xai" { self.api_keys.get("grok") } else { None })
            .cloned();
        let base_override = self
            .base_urls
            .get(provider_str)
            .cloned()
            .filter(|u| !u.trim().is_empty());

        // A stored provider_base_urls row re-points the provider at the
        // user's proxy/gateway. Only providers that are genuinely
        // OpenAI-compatible (or Anthropic-shape) support this; others keep
        // their compiled-in endpoint. Without a row the path is unchanged.
        const OPENAI_COMPAT_OVERRIDABLE: &[&str] = &[
            "openai", "deepseek", "groq", "gemini", "mistral", "together",
            "perplexity", "cohere", "opencode", "cline", "xai", "tokenrouter",
        ];
        let mut provider_impl: Box<dyn ModelProviderTrait> = match provider_str {
            "anthropic" if base_override.is_some() => Box::new(
                AnthropicProvider::new(api_key).with_base_url(base_override.unwrap()),
            ),
            s if base_override.is_some() && OPENAI_COMPAT_OVERRIDABLE.contains(&s) => Box::new(
                openai_compat::openai_compat_with_base(api_key, base_override.unwrap(), provider.clone()),
            ),
            _ => create_provider(provider, api_key, base_override),
        };

        if let Some(model) = model_id {
            if !model.trim().is_empty() {
                provider_impl = provider_impl.with_model(model.trim().to_string());
            }
        }

        provider_impl
    }

    /// Create a provider from a string name
    pub fn create_provider_from_str(&self, provider: &str) -> Result<Box<dyn ModelProviderTrait>, ModelError> {
        self.create_provider_from_str_with_model(provider, None)
    }

    /// Create a provider from a string name honoring a user model id.
    /// Unknown names fall back to the user-defined provider registry, so a
    /// `custom_providers` id resolves the same as any built-in name.
    pub fn create_provider_from_str_with_model(
        &self,
        provider: &str,
        model_id: Option<&str>,
    ) -> Result<Box<dyn ModelProviderTrait>, ModelError> {
        let lower = provider.to_lowercase();
        if let Some(spec) = self.custom.get(&lower) {
            return Ok(self.build_from_spec(spec, model_id));
        }

        let model_provider = match lower.as_str() {
            "openrouter" | "open_router" | "open-router" => ModelProvider::OpenRouter,
            "anthropic" | "claude" => ModelProvider::Anthropic,
            "openai" => ModelProvider::OpenAI,
            "ollama" => ModelProvider::Ollama,
            "local" => ModelProvider::Local,
            "commandcode" | "command_code" | "command-code" => ModelProvider::CommandCode,
            "deepseek" => ModelProvider::DeepSeek,
            "groq" => ModelProvider::Groq,
            "gemini" | "google" => ModelProvider::Gemini,
            "mistral" => ModelProvider::Mistral,
            "together" => ModelProvider::Together,
            "perplexity" => ModelProvider::Perplexity,
            "cohere" => ModelProvider::Cohere,
            "mimo" | "xiaomi" => ModelProvider::MiMo,
            "opencode" | "open_code" | "open-code" => ModelProvider::OpenCode,
            "cline" => ModelProvider::Cline,
            "xai" | "grok" => ModelProvider::XAI,
            "tokenrouter" | "token_router" | "token-router" => ModelProvider::TokenRouter,
            _ => return Err(ModelError::Provider(format!("Unknown provider: {}", provider))),
        };

        Ok(self.create_provider_with_model(model_provider, model_id))
    }

    /// Build a live provider from a user-defined spec. `model_id` (the bot's
    /// configured model) wins over the spec's default.
    fn build_from_spec(
        &self,
        spec: &CustomProviderSpec,
        model_id: Option<&str>,
    ) -> Box<dyn ModelProviderTrait> {
        let api_key = self.api_keys.get(&spec.id).cloned();
        let mut provider: Box<dyn ModelProviderTrait> = match spec.kind.as_str() {
            "anthropic" => Box::new(
                AnthropicProvider::new(api_key).with_base_url(spec.base_url.trim_end_matches('/')),
            ),
            "ollama" => Box::new(OllamaProvider::new(Some(spec.base_url.clone()))),
            _ => Box::new(openai_compat::custom_provider(
                api_key,
                spec.base_url.trim_end_matches('/'),
                "",
            )),
        };
        let effective_model = model_id
            .map(|m| m.trim().to_string())
            .filter(|m| !m.is_empty())
            .or_else(|| {
                if spec.default_model.trim().is_empty() {
                    None
                } else {
                    Some(spec.default_model.trim().to_string())
                }
            });
        if let Some(model) = effective_model {
            provider = provider.with_model(model);
        }
        provider
    }

    /// Check if a provider has an API key configured
    pub fn has_key(&self, provider: &str) -> bool {
        let p = provider.to_lowercase();
        self.api_keys.contains_key(&p)
            || (p == "claude" && self.api_keys.contains_key("anthropic"))
            || (p == "anthropic" && self.api_keys.contains_key("claude"))
            || (p == "grok" && self.api_keys.contains_key("xai"))
            || (p == "xai" && self.api_keys.contains_key("grok"))
    }

    /// Get reference to all in-memory API keys
    pub fn get_api_keys(&self) -> &HashMap<String, String> {
        &self.api_keys
    }

    /// Get list of configured provider IDs
    pub fn configured_providers(&self) -> Vec<String> {
        let mut list: Vec<String> = self.api_keys.keys().cloned().collect();
        // If anthropic is present, also add claude so UI keyed by either works
        if self.api_keys.contains_key("anthropic") && !list.contains(&"claude".to_string()) {
            list.push("claude".to_string());
        }
        if self.api_keys.contains_key("xai") && !list.contains(&"grok".to_string()) {
            list.push("grok".to_string());
        }
        list.sort();
        list.dedup();
        list
    }
}

impl Default for ProviderManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Names (and aliases) that resolve to built-in providers — a custom
/// provider id must never shadow these.
pub fn is_reserved_provider_name(name: &str) -> bool {
    matches!(
        name.to_lowercase().as_str(),
        "openrouter" | "open_router" | "open-router"
            | "anthropic" | "claude"
            | "openai"
            | "ollama"
            | "local"
            | "commandcode" | "command_code" | "command-code"
            | "deepseek"
            | "groq"
            | "gemini" | "google"
            | "mistral"
            | "together"
            | "perplexity"
            | "cohere"
            | "mimo" | "xiaomi"
            | "opencode" | "open_code" | "open-code"
            | "cline"
            | "xai" | "grok"
            | "tokenrouter" | "token_router" | "token-router"
            | "custom"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenrouter_is_a_known_provider() {
        let manager = ProviderManager::new();
        let provider = manager
            .create_provider_from_str_with_model("tokenrouter", Some("z-ai/glm-5.3"))
            .expect("tokenrouter must resolve through the provider manager");
        assert_eq!(provider.provider_type(), ModelProvider::TokenRouter);
        assert_eq!(ModelProvider::TokenRouter.display_name(), "TokenRouter");
    }

    #[test]
    fn tokenrouter_alias_spellings_resolve() {
        let manager = ProviderManager::new();
        for name in ["tokenrouter", "token_router", "token-router", "TokenRouter"] {
            assert!(
                manager.create_provider_from_str(name).is_ok(),
                "alias {name} should resolve"
            );
        }
    }

    fn sample_spec(kind: &str) -> CustomProviderSpec {
        CustomProviderSpec {
            id: "lmstudio".to_string(),
            display_name: "LM Studio".to_string(),
            kind: kind.to_string(),
            base_url: "http://192.168.1.10:1234/v1".to_string(),
            default_model: "qwen/qwen3-8b".to_string(),
            supports_tools: true,
        }
    }

    #[test]
    fn custom_provider_resolves_for_each_kind() {
        for kind in ["openai", "anthropic", "ollama"] {
            let mut manager = ProviderManager::new();
            manager.register_custom(sample_spec(kind));
            let provider = manager
                .create_provider_from_str_with_model("lmstudio", None)
                .unwrap_or_else(|e| panic!("{kind} custom provider must resolve: {e}"));
            // ollama/anthropic kinds reuse the built-in implementations
            // (so provider_type reports theirs); openai kinds are Custom.
            let expected = match kind {
                "ollama" => ModelProvider::Ollama,
                "anthropic" => ModelProvider::Anthropic,
                _ => ModelProvider::Custom,
            };
            assert_eq!(provider.provider_type(), expected, "kind {kind}");
        }
    }

    #[test]
    fn custom_provider_name_is_case_insensitive_and_scoped() {
        let mut manager = ProviderManager::new();
        manager.register_custom(sample_spec("openai"));
        assert!(manager.create_provider_from_str("LMStudio").is_ok());
        // Without the registration the same name is still unknown
        let bare = ProviderManager::new();
        assert!(bare.create_provider_from_str("lmstudio").is_err());
    }

    #[test]
    fn reserved_names_reject_custom_shadowing() {
        for name in ["openai", "claude", "grok", "xai", "anthropic", "OpenRouter", "token-router", "ollama", "local"] {
            assert!(is_reserved_provider_name(name), "{name} must be reserved");
        }
        assert!(!is_reserved_provider_name("lmstudio"));
        assert!(!is_reserved_provider_name("my-gateway"));
    }

    #[test]
    fn builtins_resolve_identically_with_and_without_base_override() {
        let plain = ProviderManager::new();
        let mut overridden = ProviderManager::new();
        overridden.set_base_url("deepseek", "http://proxy.local:8000/v1".to_string());
        for name in ["deepseek", "groq", "anthropic", "tokenrouter"] {
            let a = plain.create_provider_from_str(name).unwrap();
            let b = overridden.create_provider_from_str(name).unwrap();
            assert_eq!(a.provider_type(), b.provider_type(), "{name} type must not drift");
        }
        // Providers outside the override-capable family ignore the row
        let mut weird = ProviderManager::new();
        weird.set_base_url("openrouter", "http://proxy.local:8000/v1".to_string());
        assert_eq!(
            weird.create_provider_from_str("openrouter").unwrap().provider_type(),
            ModelProvider::OpenRouter
        );
    }
}

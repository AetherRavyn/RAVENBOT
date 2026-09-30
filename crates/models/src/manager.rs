//! Model provider manager
//!
//! Manages API keys and creates model providers on demand.

use std::collections::HashMap;
use ravenbot_core::ModelProvider;

use super::{ModelProviderTrait, ModelError, create_provider};

pub struct ProviderManager {
    api_keys: HashMap<String, String>,
    base_urls: HashMap<String, String>,
}

impl ProviderManager {
    pub fn new() -> Self {
        let mut api_keys = HashMap::new();
        let mut base_urls = HashMap::new();

        // Check environment variables for default keys
        if let Ok(k) = std::env::var("OPENROUTER_API_KEY") {
            if !k.trim().is_empty() {
                api_keys.insert("openrouter".to_string(), k.trim().to_string());
            }
        }
        if let Ok(k) = std::env::var("ANTHROPIC_API_KEY") {
            if !k.trim().is_empty() {
                api_keys.insert("anthropic".to_string(), k.trim().to_string());
            }
        }
        if let Ok(k) = std::env::var("OPENAI_API_KEY") {
            if !k.trim().is_empty() {
                api_keys.insert("openai".to_string(), k.trim().to_string());
            }
        }
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
        }
    }

    /// Set an API key for a provider
    pub fn set_api_key(&mut self, provider: &str, key: String) {
        self.api_keys.insert(provider.to_string(), key);
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
        };

        let api_key = self.api_keys.get(provider_str).cloned();
        let base_url = self.base_urls.get(provider_str).cloned();

        let mut provider_impl = create_provider(provider, api_key, base_url);

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

    /// Create a provider from a string name honoring a user model id
    pub fn create_provider_from_str_with_model(
        &self,
        provider: &str,
        model_id: Option<&str>,
    ) -> Result<Box<dyn ModelProviderTrait>, ModelError> {
        let model_provider = match provider.to_lowercase().as_str() {
            "openrouter" => ModelProvider::OpenRouter,
            "anthropic" => ModelProvider::Anthropic,
            "openai" => ModelProvider::OpenAI,
            "ollama" => ModelProvider::Ollama,
            "local" => ModelProvider::Local,
            _ => return Err(ModelError::Provider(format!("Unknown provider: {}", provider))),
        };

        Ok(self.create_provider_with_model(model_provider, model_id))
    }

    /// Check if a provider has an API key configured
    pub fn has_key(&self, provider: &str) -> bool {
        self.api_keys.contains_key(provider)
    }
}

impl Default for ProviderManager {
    fn default() -> Self {
        Self::new()
    }
}

//! Dynamic model discovery — fetch available models from each provider's
//! `/models` endpoint, cache with TTL, and detect free models.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::ModelError;

/// A discovered model from a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredModel {
    /// Model ID (e.g. "claude-3-5-sonnet-20241022")
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Context window size in tokens
    pub context_window: u32,
    /// Cost per 1M input tokens (0 = free)
    pub input_cost_per_1m: f64,
    /// Cost per 1M output tokens (0 = free)
    pub output_cost_per_1m: f64,
    /// Supports vision/image input
    pub supports_vision: bool,
    /// Supports tool/function calling
    pub supports_tools: bool,
    /// Whether this model is free
    pub is_free: bool,
}

/// Provider-specific model list endpoint configuration.
#[derive(Debug, Clone)]
pub struct ProviderEndpoint {
    base_url: String,
    /// Env var holding this provider's key (informational — key lookup is by
    /// provider id at request time).
    #[allow(dead_code)]
    api_key_env: String,
    /// Path appended to base_url (e.g. "/models")
    path: String,
    /// How to extract models from the response
    extractor: ModelExtractor,
    /// How to authenticate the discovery request
    auth_style: AuthStyle,
}

/// Auth header shape a provider's /models endpoint expects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuthStyle {
    /// `Authorization: Bearer <key>`
    Bearer,
    /// `x-api-key` + `anthropic-version`
    XApiKey,
    /// Bearer plus Google's `x-goog-api-key` (Gemini accepts both)
    BearerAndGoogleKey,
    /// No key required (local daemons)
    Keyless,
}

/// How to extract model list from a provider's /models response.
#[derive(Debug, Clone)]
enum ModelExtractor {
    /// OpenAI format: { "data": [ { "id": "...", "context_length": N } ] }
    OpenAIFormat,
    /// Anthropic format: { "data": [ { "id": "...", "display_name": "..." } ] }
    AnthropicFormat,
    /// Ollama format: { "models": [ { "name": "...", "details": {...} } ] }
    /// No API key required; queried against the local daemon.
    OllamaFormat,
}

impl ProviderEndpoint {
    fn openai(base_url: &str, api_key_env: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key_env: api_key_env.to_string(),
            path: "/models".to_string(),
            extractor: ModelExtractor::OpenAIFormat,
            auth_style: AuthStyle::Bearer,
        }
    }

    fn anthropic(base_url: &str, api_key_env: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key_env: api_key_env.to_string(),
            path: "/models".to_string(),
            extractor: ModelExtractor::AnthropicFormat,
            auth_style: AuthStyle::XApiKey,
        }
    }

    /// Endpoint for a user-defined provider kind ("openai" | "anthropic" |
    /// "ollama") pointed at an arbitrary base URL.
    fn for_kind(kind: &str, base_url: &str) -> Result<Self, ModelError> {
        match kind {
            "anthropic" => Ok(Self::anthropic(base_url, "")),
            "ollama" => {
                let base = if base_url.trim().is_empty() {
                    ollama_base_url()
                } else {
                    base_url.trim().trim_end_matches('/').to_string()
                };
                Ok(Self {
                    base_url: base,
                    api_key_env: String::new(),
                    path: "/api/tags".to_string(),
                    extractor: ModelExtractor::OllamaFormat,
                    auth_style: AuthStyle::Keyless,
                })
            }
            "openai" | "" => Ok(Self::openai(base_url, "")),
            other => Err(ModelError::Provider(format!(
                "Unknown custom provider kind: {other}"
            ))),
        }
    }
}

/// Get all provider endpoints that support model discovery.
pub fn provider_endpoints() -> HashMap<String, ProviderEndpoint> {
    let mut map = HashMap::new();

    // OpenAI-compatible providers
    for (name, base, key) in &[
        ("openrouter", "https://openrouter.ai/api/v1", "OPENROUTER_API_KEY"),
        ("openai", "https://api.openai.com/v1", "OPENAI_API_KEY"),
        ("deepseek", "https://api.deepseek.com/v1", "DEEPSEEK_API_KEY"),
        ("groq", "https://api.groq.com/openai/v1", "GROQ_API_KEY"),
        ("gemini", "https://generativelanguage.googleapis.com/v1beta/openai", "GEMINI_API_KEY"),
        ("mistral", "https://api.mistral.ai/v1", "MISTRAL_API_KEY"),
        ("together", "https://api.together.xyz/v1", "TOGETHER_API_KEY"),
        ("perplexity", "https://api.perplexity.ai", "PERPLEXITY_API_KEY"),
        ("cohere", "https://api.cohere.com/compatibility/v1", "COHERE_API_KEY"),
        ("mimo", "https://api.mimo.mi.com/v1", "MIMO_API_KEY"),
        ("commandcode", "https://api.commandcode.ai/provider/v1", "COMMANDCODE_API_KEY"),
        ("xai", "https://api.x.ai/v1", "XAI_API_KEY"),
        ("opencode", "https://opencode.ai/zen/v1", "OPENCODE_API_KEY"),
        ("cline", "https://api.cline.bot/api/v1", "CLINE_API_KEY"),
        ("tokenrouter", "https://api.tokenrouter.com/v1", "TOKENROUTER_API_KEY"),
    ] {
        map.insert(name.to_string(), ProviderEndpoint::openai(base, key));
    }
    // Gemini accepts both auth shapes on its OpenAI-compat surface.
    if let Some(gemini) = map.get_mut("gemini") {
        gemini.auth_style = AuthStyle::BearerAndGoogleKey;
    }

    // Anthropic direct (also aliased to "claude")
    map.insert(
        "anthropic".to_string(),
        ProviderEndpoint::anthropic("https://api.anthropic.com/v1", "ANTHROPIC_API_KEY"),
    );
    map.insert(
        "claude".to_string(),
        ProviderEndpoint::anthropic("https://api.anthropic.com/v1", "ANTHROPIC_API_KEY"),
    );

    // Ollama local daemon — keyless, queried on the machine's Ollama host.
    // The base URL is resolved per-request (env override aware), so the
    // stored endpoint just needs the right extractor; the path is /api/tags.
    map.insert(
        "ollama".to_string(),
        ProviderEndpoint {
            base_url: String::new(), // resolved dynamically in fetch_from_provider
            api_key_env: String::new(),
            path: "/api/tags".to_string(),
            extractor: ModelExtractor::OllamaFormat,
            auth_style: AuthStyle::Keyless,
        },
    );

    map
}

/// Resolve the Ollama base URL (env override or default).
fn ollama_base_url() -> String {
    for var in &["OLLAMA_HOST", "OLLAMA_URL"] {
        if let Ok(u) = std::env::var(var) {
            let u = u.trim().trim_end_matches('/').to_string();
            if !u.is_empty() {
                return u;
            }
        }
    }
    "http://localhost:11434".to_string()
}

/// Cache entry for a provider's model list.
#[derive(Debug, Clone)]
struct CacheEntry {
    models: Vec<DiscoveredModel>,
    fetched_at: Instant,
}

/// Model discovery service with TTL caching.
pub struct ModelDiscovery {
    cache: Arc<Mutex<HashMap<String, CacheEntry>>>,
    ttl: Duration,
    client: reqwest::Client,
}

impl ModelDiscovery {
    /// Create a new discovery service with the given TTL.
    pub fn new(ttl: Duration) -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
            ttl,
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Create with default 1-hour TTL.
    pub fn default() -> Self {
        Self::new(Duration::from_secs(3600))
    }

    /// Fetch models for a provider, using cache if fresh.
    /// Set `force_refresh = true` to bypass cache.
    pub async fn fetch_models(
        &self,
        provider: &str,
        api_key: Option<&str>,
        force_refresh: bool,
    ) -> Result<Vec<DiscoveredModel>, ModelError> {
        // Check cache first
        if !force_refresh {
            let cache = self.cache.lock().await;
            if let Some(entry) = cache.get(provider) {
                if entry.fetched_at.elapsed() < self.ttl {
                    return Ok(entry.models.clone());
                }
            }
        }

        // Fetch from provider
        let models = self.fetch_from_provider(provider, api_key).await?;

        // Update cache
        let mut cache = self.cache.lock().await;
        cache.insert(provider.to_string(), CacheEntry {
            models: models.clone(),
            fetched_at: Instant::now(),
        });

        Ok(models)
    }

    /// Fetch all providers' models in parallel.
    pub async fn fetch_all_providers(
        &self,
        api_keys: &HashMap<String, String>,
    ) -> HashMap<String, Vec<DiscoveredModel>> {
        let endpoints = provider_endpoints();
        let mut results = HashMap::new();

        let futures: Vec<_> = endpoints
            .iter()
            .map(|(name, _endpoint)| {
                // NOTE: `api_keys` is keyed by provider id (e.g. "openrouter"),
                // not by env-var name — look up by provider id.
                let api_key = api_keys.get(name).cloned();
                let name = name.clone();
                async move {
                    let models = self.fetch_models(&name, api_key.as_deref(), false).await;
                    (name, models)
                }
            })
            .collect();

        // Execute all requests concurrently
        for (name, result) in futures::future::join_all(futures).await {
            if let Ok(models) = result {
                if !models.is_empty() {
                    results.insert(name, models);
                }
            }
        }

        results
    }

    /// Fetch models for a user-defined provider synthesized from a base URL
    /// and kind ("openai" | "anthropic" | "ollama"). Results are cached under
    /// `provider` (the custom id).
    pub async fn fetch_models_for(
        &self,
        provider: &str,
        api_key: Option<&str>,
        base_url: &str,
        kind: &str,
    ) -> Result<Vec<DiscoveredModel>, ModelError> {
        let endpoint = ProviderEndpoint::for_kind(kind, base_url)?;
        let models = self.fetch_with_endpoint(provider, api_key, &endpoint).await?;
        let mut cache = self.cache.lock().await;
        cache.insert(provider.to_string(), CacheEntry {
            models: models.clone(),
            fetched_at: Instant::now(),
        });
        Ok(models)
    }

    async fn fetch_from_provider(
        &self,
        provider: &str,
        api_key: Option<&str>,
    ) -> Result<Vec<DiscoveredModel>, ModelError> {
        let endpoints = provider_endpoints();
        let endpoint = endpoints
            .get(provider)
            .ok_or_else(|| ModelError::Provider(format!("No discovery endpoint for: {}", provider)))?;
        self.fetch_with_endpoint(provider, api_key, endpoint).await
    }

    async fn fetch_with_endpoint(
        &self,
        provider: &str,
        api_key: Option<&str>,
        endpoint: &ProviderEndpoint,
    ) -> Result<Vec<DiscoveredModel>, ModelError> {
        let api_key = if endpoint.auth_style == AuthStyle::Keyless {
            api_key.map(|k| k.to_string())
        } else {
            Some(
                api_key
                    .ok_or_else(|| ModelError::Auth(format!("No API key for: {}", provider)))?
                    .to_string(),
            )
        };

        let base = if endpoint.base_url.is_empty() {
            ollama_base_url()
        } else {
            endpoint.base_url.clone()
        };
        let url = format!("{}{}", base, endpoint.path);

        let mut req = self.client.get(&url);
        if let Some(key) = &api_key {
            match endpoint.auth_style {
                AuthStyle::XApiKey => {
                    req = req.header("x-api-key", key);
                    req = req.header("anthropic-version", "2023-06-01");
                }
                AuthStyle::BearerAndGoogleKey => {
                    req = req.header("Authorization", format!("Bearer {}", key));
                    req = req.header("x-goog-api-key", key);
                }
                AuthStyle::Bearer => {
                    req = req.header("Authorization", format!("Bearer {}", key));
                }
                AuthStyle::Keyless => {}
            }
        }
        let response = req.send().await.map_err(ModelError::Http)?;

        if !response.status().is_success() {
            return Err(ModelError::Provider(format!(
                "Failed to fetch models from {}: {}",
                provider,
                response.status()
            )));
        }

        let json: serde_json::Value = response.json().await.map_err(ModelError::Http)?;

        // Extract models based on format
        match &endpoint.extractor {
            ModelExtractor::OpenAIFormat => Self::extract_openai_format(&json, provider),
            ModelExtractor::AnthropicFormat => Self::extract_anthropic_format(&json, provider),
            ModelExtractor::OllamaFormat => Self::extract_ollama_format(&json, provider),
        }
    }

    fn extract_openai_format(
        json: &serde_json::Value,
        provider: &str,
    ) -> Result<Vec<DiscoveredModel>, ModelError> {
        let data = json
            .get("data")
            .and_then(|d| d.as_array())
            .ok_or_else(|| ModelError::Provider(format!("Unexpected response format from {}", provider)))?;

        let mut models = Vec::new();
        for item in data {
            let id = item
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if id.is_empty() {
                continue;
            }

            let name = item
                .get("name")
                .and_then(|v| v.as_str())
                .or_else(|| item.get("id").and_then(|v| v.as_str()))
                .unwrap_or(&id)
                .to_string();

            let context_window = item
                .get("context_length")
                .or_else(|| item.get("context_window"))
                .or_else(|| item.get("max_context_length"))
                .or_else(|| item.get("contextLength"))
                .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok())))
                .unwrap_or(8192) as u32;

            // Extract pricing if available (string or numeric shapes).
            let cost_of = |keys: &[&str]| -> f64 {
                let pricing = item.get("pricing");
                for key in keys {
                    if let Some(v) = pricing.and_then(|p| p.get(*key)) {
                        if let Some(s) = v.as_str() {
                            if let Ok(n) = s.parse::<f64>() {
                                return n;
                            }
                        }
                        if let Some(n) = v.as_f64() {
                            return n;
                        }
                        if let Some(n) = v.as_u64() {
                            return n as f64;
                        }
                    }
                }
                // Some providers (Together, Groq) flatten pricing differently.
                for key in &["prompt_price", "input_price_per_token", "price_input"] {
                    if let Some(n) = item.get(*key).and_then(|v| v.as_f64()) {
                        if keys[0].contains("input") || keys[0].contains("prompt") {
                            return n;
                        }
                    }
                }
                for key in &["completion_price", "output_price_per_token", "price_output"] {
                    if let Some(n) = item.get(*key).and_then(|v| v.as_f64()) {
                        if keys[0].contains("output") || keys[0].contains("completion") {
                            return n;
                        }
                    }
                }
                0.0
            };
            let input_cost = cost_of(&["input", "prompt", "prompt_tokens"]);
            let output_cost = cost_of(&["output", "completion", "completion_tokens"]);

            let is_free = input_cost == 0.0 && output_cost == 0.0;

            let supports_vision = item
                .get("architecture")
                .and_then(|a| a.get("modality"))
                .and_then(|m| m.as_str())
                .map(|m| m.contains("image") || m.contains("vision"))
                .unwrap_or_else(|| {
                    id.to_lowercase().contains("vision")
                        || id.to_lowercase().contains("gpt-4o")
                        || id.to_lowercase().contains("gemini")
                        || id.to_lowercase().contains("claude")
                        || id.to_lowercase().contains("pixtral")
                });

            let supports_tools = item
                .get("architecture")
                .and_then(|a| a.get("instruct_type"))
                .is_some()
                || !id.contains("instruct-only");

            models.push(DiscoveredModel {
                id,
                name,
                context_window,
                input_cost_per_1m: input_cost,
                output_cost_per_1m: output_cost,
                supports_vision,
                supports_tools,
                is_free,
            });
        }

        // Sort: free models first, then by name
        models.sort_by(|a, b| {
            b.is_free.cmp(&a.is_free)
                .then_with(|| a.name.cmp(&b.name))
        });

        Ok(models)
    }

    fn extract_anthropic_format(
        json: &serde_json::Value,
        provider: &str,
    ) -> Result<Vec<DiscoveredModel>, ModelError> {
        let data = json
            .get("data")
            .and_then(|d| d.as_array())
            .ok_or_else(|| ModelError::Provider(format!("Unexpected response format from {}", provider)))?;

        let mut models = Vec::new();
        for item in data {
            let id = item
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if id.is_empty() {
                continue;
            }

            let display_name = item
                .get("display_name")
                .and_then(|v| v.as_str())
                .unwrap_or(&id);

            models.push(DiscoveredModel {
                id: id.clone(),
                name: display_name.to_string(),
                context_window: 200000,
                input_cost_per_1m: 0.0,
                output_cost_per_1m: 0.0,
                supports_vision: true,
                supports_tools: true,
                is_free: false,
            });
        }

        models.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(models)
    }

    fn extract_ollama_format(
        json: &serde_json::Value,
        provider: &str,
    ) -> Result<Vec<DiscoveredModel>, ModelError> {
        let list = json
            .get("models")
            .and_then(|m| m.as_array())
            .ok_or_else(|| ModelError::Provider(format!("Unexpected response format from {}", provider)))?;

        let mut models = Vec::new();
        for item in list {
            let id = item
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if id.is_empty() {
                continue;
            }
            let details = item.get("details");
            let family = details
                .and_then(|d| d.get("family"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_lowercase();
            let parameter_size = details
                .and_then(|d| d.get("parameter_size"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let name = if parameter_size.is_empty() {
                id.clone()
            } else {
                format!("{} ({})", id, parameter_size)
            };
            // Ollama vision-capable families (llava, llama-vision, moondream…)
            let lower = format!("{} {}", id.to_lowercase(), family);
            let supports_vision = ["llava", "vision", "moondream", "bakllava", "qwen2-vl"]
                .iter()
                .any(|k| lower.contains(k));
            models.push(DiscoveredModel {
                id: id.clone(),
                name,
                context_window: 8192,
                input_cost_per_1m: 0.0,
                output_cost_per_1m: 0.0,
                supports_vision,
                supports_tools: true,
                is_free: true,
            });
        }

        models.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(models)
    }

    /// Clear the cache for a specific provider (or all if None).
    pub async fn clear_cache(&self, provider: Option<&str>) {
        let mut cache = self.cache.lock().await;
        if let Some(p) = provider {
            cache.remove(p);
        } else {
            cache.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_endpoints_have_correct_opencode_and_cline_urls() {
        let endpoints = provider_endpoints();
        let opencode = endpoints.get("opencode").expect("opencode endpoint must exist");
        assert_eq!(opencode.base_url, "https://opencode.ai/zen/v1");
        assert_eq!(opencode.path, "/models");

        let cline = endpoints.get("cline").expect("cline endpoint must exist");
        assert_eq!(cline.base_url, "https://api.cline.bot/api/v1");
        assert_eq!(cline.path, "/models");

        let tr = endpoints.get("tokenrouter").expect("tokenrouter endpoint must exist");
        assert_eq!(tr.base_url, "https://api.tokenrouter.com/v1");
        assert_eq!(tr.path, "/models");
    }

    #[test]
    fn test_extract_openai_format_opencode_sample() {
        let json = serde_json::json!({
            "object": "list",
            "data": [
                {
                    "id": "claude-sonnet-4-5",
                    "object": "model",
                    "owned_by": "opencode"
                },
                {
                    "id": "gpt-5",
                    "object": "model",
                    "owned_by": "opencode"
                }
            ]
        });

        let models = ModelDiscovery::extract_openai_format(&json, "opencode").expect("must extract models");
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "claude-sonnet-4-5");
        assert_eq!(models[1].id, "gpt-5");
    }
}




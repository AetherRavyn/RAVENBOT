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
    /// Emits chain-of-thought / extended thinking
    #[serde(default)]
    pub reasoning: bool,
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
    /// When true, absence of pricing data does NOT imply the model is free
    /// (gateways like Command Code omit pricing entirely — only an explicit
    /// free-shaped id counts).
    free_by_id_only: bool,
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
    /// Bearer if a key exists, but the endpoint answers keyless
    /// (Command Code's /models is public).
    BearerOptional,
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
            free_by_id_only: false,
        }
    }

    fn anthropic(base_url: &str, api_key_env: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key_env: api_key_env.to_string(),
            path: "/models".to_string(),
            extractor: ModelExtractor::AnthropicFormat,
            auth_style: AuthStyle::XApiKey,
            free_by_id_only: false,
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
                    free_by_id_only: false,
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
        (
            "openrouter",
            "https://openrouter.ai/api/v1",
            "OPENROUTER_API_KEY",
        ),
        ("openai", "https://api.openai.com/v1", "OPENAI_API_KEY"),
        (
            "deepseek",
            "https://api.deepseek.com/v1",
            "DEEPSEEK_API_KEY",
        ),
        ("groq", "https://api.groq.com/openai/v1", "GROQ_API_KEY"),
        (
            "gemini",
            "https://generativelanguage.googleapis.com/v1beta/openai",
            "GEMINI_API_KEY",
        ),
        ("mistral", "https://api.mistral.ai/v1", "MISTRAL_API_KEY"),
        (
            "together",
            "https://api.together.xyz/v1",
            "TOGETHER_API_KEY",
        ),
        (
            "perplexity",
            "https://api.perplexity.ai",
            "PERPLEXITY_API_KEY",
        ),
        (
            "cohere",
            "https://api.cohere.com/compatibility/v1",
            "COHERE_API_KEY",
        ),
        ("mimo", "https://api.mimo.mi.com/v1", "MIMO_API_KEY"),
        ("xai", "https://api.x.ai/v1", "XAI_API_KEY"),
        ("opencode", "https://opencode.ai/zen/v1", "OPENCODE_API_KEY"),
        // OpenCode Go's model list lives on the subscription host, not on the
        // OpenAI-compatible chat base (`…/go/openai/v1/models` is a 404).
        (
            "opencode-go",
            "https://opencode.ai/inference/go/v1",
            "OPENCODE_GO_API_KEY",
        ),
        ("cline", "https://api.cline.bot/api/v1", "CLINE_API_KEY"),
        (
            "tokenrouter",
            "https://api.tokenrouter.com/v1",
            "TOKENROUTER_API_KEY",
        ),
    ] {
        map.insert(name.to_string(), ProviderEndpoint::openai(base, key));
    }
    // Gemini accepts both auth shapes on its OpenAI-compat surface.
    if let Some(gemini) = map.get_mut("gemini") {
        gemini.auth_style = AuthStyle::BearerAndGoogleKey;
    }
    // Go's /models response carries no pricing. Only an explicitly free-shaped
    // id is free until models.dev metadata is merged in.
    if let Some(go) = map.get_mut("opencode-go") {
        go.free_by_id_only = true;
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

    // Command Code gateway: /provider/v1/models is public (key optional),
    // and its entries carry no pricing fields — free models are only the
    // ones with a free-shaped id (`…:free`, `…-free`, `stealth/…`).
    map.insert(
        "commandcode".to_string(),
        ProviderEndpoint {
            base_url: "https://api.commandcode.ai/provider/v1".to_string(),
            api_key_env: "COMMANDCODE_API_KEY".to_string(),
            path: "/models".to_string(),
            extractor: ModelExtractor::OpenAIFormat,
            auth_style: AuthStyle::BearerOptional,
            free_by_id_only: true,
        },
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
            free_by_id_only: false,
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

// ── OpenCode Go metadata ────────────────────────────────────────────────────

/// One model's facts from `models.dev/api.json`.
///
/// The live Go `/models` endpoint returns ids only. models.dev is the registry
/// OpenCode itself uses, and it carries the name, context window, price,
/// capability flags — and, crucially, which wire protocol the model is served
/// over — for every Go model. Using it is what makes the list self-updating
/// without a hardcoded table in this repository.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct GoModelMeta {
    name: String,
    context_window: u32,
    input_cost_micros: u64,
    output_cost_micros: u64,
    supports_vision: bool,
    supports_tools: bool,
    reasoning: bool,
    /// "openai" | "anthropic" | "responses" | "google" | ""
    protocol: String,
}

fn protocol_from_npm(npm: &str) -> &'static str {
    match npm {
        "@ai-sdk/openai-compatible" => "openai",
        "@ai-sdk/anthropic" => "anthropic",
        "@ai-sdk/openai" => "responses",
        "@ai-sdk/google" => "google",
        _ => "",
    }
}

/// Parse the `opencode-go` slice of models.dev. Pure so it is testable without
/// a network call.
fn parse_go_meta(json: &serde_json::Value) -> HashMap<String, GoModelMeta> {
    let Some(models) = json
        .get("opencode-go")
        .and_then(|p| p.get("models"))
        .and_then(|m| m.as_object())
    else {
        return HashMap::new();
    };

    let mut out = HashMap::with_capacity(models.len());
    for (id, m) in models {
        let cost = m.get("cost");
        let limit = m.get("limit");
        let micros = |v: Option<&serde_json::Value>| -> u64 {
            v.and_then(|n| n.as_f64())
                .filter(|n| n.is_finite() && *n >= 0.0)
                .map(|n| (n * 1_000_000.0).round() as u64)
                .unwrap_or(0)
        };
        let vision = m
            .get("attachment")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
            || m.pointer("/modalities/input")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().any(|x| x.as_str() == Some("image")))
                .unwrap_or(false);
        out.insert(
            id.clone(),
            GoModelMeta {
                name: m
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or(id)
                    .to_string(),
                context_window: limit
                    .and_then(|l| l.get("context"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32,
                input_cost_micros: micros(cost.and_then(|c| c.get("input"))),
                output_cost_micros: micros(cost.and_then(|c| c.get("output"))),
                supports_vision: vision,
                supports_tools: m.get("tool_call").and_then(|v| v.as_bool()).unwrap_or(true),
                reasoning: m
                    .get("reasoning")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                protocol: m
                    .pointer("/provider/npm")
                    .and_then(|v| v.as_str())
                    .map(protocol_from_npm)
                    .unwrap_or("")
                    .to_string(),
            },
        );
    }
    out
}

/// Wire protocol for a live model id.
///
/// Exact metadata wins. When a model is newer than the models.dev snapshot, its
/// family gives the answer: `grok-4.6` has no entry yet, but every `grok-*`
/// sibling is served over `/responses`, and `glm-5.1` belongs to an
/// OpenAI-compatible family. This is metadata-derived, not a vendored list:
/// adding a new Grok to models.dev changes the answer automatically.
fn protocol_for<'a>(id: &str, meta: &'a HashMap<String, GoModelMeta>) -> Option<&'a str> {
    if let Some(m) = meta.get(id) {
        if !m.protocol.is_empty() {
            return Some(m.protocol.as_str());
        }
    }
    let family = id.split('-').next().unwrap_or(id);
    meta.iter()
        .find(|(key, value)| key.split('-').next() == Some(family) && !value.protocol.is_empty())
        .map(|(_, value)| value.protocol.as_str())
}

/// Apply models.dev facts to the live id list and remove models this client
/// cannot speak to.
///
/// `/responses` (GPT/Grok/Muse) and Google's native protocol have no adapter in
/// RAVENBOT yet. Offering them would produce a `ModelProtocolUnsupported` error
/// after the user picked one; hiding them is the honest option. Claude is kept
/// because [`crate::opencode_go::OpenCodeGoProvider`] routes it to the Anthropic
/// endpoint.
fn apply_go_meta(
    mut models: Vec<DiscoveredModel>,
    meta: &HashMap<String, GoModelMeta>,
) -> Vec<DiscoveredModel> {
    if meta.is_empty() {
        // Metadata unreachable. The live ids still work for the majority
        // (everything except `/responses` and Google models); return them with
        // honest "unknown" pricing rather than an empty list.
        for m in &mut models {
            m.input_cost_per_1m = 0.0;
            m.output_cost_per_1m = 0.0;
        }
        return models;
    }

    models.retain(|m| {
        !matches!(
            protocol_for(&m.id, meta),
            Some("responses") | Some("google")
        )
    });

    for m in &mut models {
        let Some(x) = meta.get(&m.id) else { continue };
        if !x.name.is_empty() {
            m.name = x.name.clone();
        }
        if x.context_window > 0 {
            m.context_window = x.context_window;
        }
        m.input_cost_per_1m = x.input_cost_micros as f64 / 1_000_000.0;
        m.output_cost_per_1m = x.output_cost_micros as f64 / 1_000_000.0;
        m.supports_tools = x.supports_tools;
        m.supports_vision = x.supports_vision;
        m.reasoning = x.reasoning;
        m.is_free = x.input_cost_micros == 0 && x.output_cost_micros == 0;
    }

    models.sort_by(|a, b| b.is_free.cmp(&a.is_free).then_with(|| a.name.cmp(&b.name)));
    models
}

/// `(fetched_at, metadata)` held behind one lock.
type GoMetaCache = Arc<Mutex<Option<(Instant, HashMap<String, GoModelMeta>)>>>;

/// Model discovery service with TTL caching.
pub struct ModelDiscovery {
    cache: Arc<Mutex<HashMap<String, CacheEntry>>>,
    ttl: Duration,
    client: reqwest::Client,
    /// models.dev's OpenCode Go slice, cached separately: it changes far less
    /// often than a provider's live id list and the document is several MB.
    go_meta: GoMetaCache,
}

impl ModelDiscovery {
    /// Create a new discovery service with the given TTL.
    pub fn new(ttl: Duration) -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
            ttl,
            client: reqwest::Client::builder()
                .user_agent("ravenbot/0.2.0")
                .timeout(Duration::from_secs(15))
                .build()
                .unwrap_or_default(),
            go_meta: Arc::new(Mutex::new(None)),
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
        cache.insert(
            provider.to_string(),
            CacheEntry {
                models: models.clone(),
                fetched_at: Instant::now(),
            },
        );

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
        let models = self
            .fetch_with_endpoint(provider, api_key, &endpoint)
            .await?;
        let mut cache = self.cache.lock().await;
        cache.insert(
            provider.to_string(),
            CacheEntry {
                models: models.clone(),
                fetched_at: Instant::now(),
            },
        );
        Ok(models)
    }

    async fn fetch_from_provider(
        &self,
        provider: &str,
        api_key: Option<&str>,
    ) -> Result<Vec<DiscoveredModel>, ModelError> {
        let endpoints = provider_endpoints();
        let endpoint = endpoints.get(provider).ok_or_else(|| {
            ModelError::Provider(format!("No discovery endpoint for: {}", provider))
        })?;
        let models = self
            .fetch_with_endpoint(provider, api_key, endpoint)
            .await?;
        if provider == "opencode-go" {
            let meta = self.opencode_go_meta().await;
            return Ok(apply_go_meta(models, &meta));
        }
        Ok(models)
    }

    /// Load and cache models.dev's OpenCode Go metadata (6 hour TTL).
    ///
    /// Failure is not fatal: the caller falls back to the live id list. The
    /// cache is only populated on success, so a transient network error does
    /// not pin an empty table for six hours.
    async fn opencode_go_meta(&self) -> HashMap<String, GoModelMeta> {
        {
            let guard = self.go_meta.lock().await;
            if let Some((fetched, meta)) = &*guard {
                if fetched.elapsed() < Duration::from_secs(6 * 3600) {
                    return meta.clone();
                }
            }
        }

        let response = match self.client.get("https://models.dev/api.json").send().await {
            Ok(r) if r.status().is_success() => r,
            _ => return HashMap::new(),
        };
        let json: serde_json::Value = match response.json().await {
            Ok(j) => j,
            Err(_) => return HashMap::new(),
        };
        let meta = parse_go_meta(&json);
        if meta.is_empty() {
            return meta;
        }
        let mut guard = self.go_meta.lock().await;
        *guard = Some((Instant::now(), meta.clone()));
        meta
    }

    async fn fetch_with_endpoint(
        &self,
        provider: &str,
        api_key: Option<&str>,
        endpoint: &ProviderEndpoint,
    ) -> Result<Vec<DiscoveredModel>, ModelError> {
        let api_key = if matches!(
            endpoint.auth_style,
            AuthStyle::Keyless | AuthStyle::BearerOptional
        ) {
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
                AuthStyle::BearerOptional => {
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
            ModelExtractor::OpenAIFormat => {
                Self::extract_openai_format(&json, provider, endpoint.free_by_id_only)
            }
            ModelExtractor::AnthropicFormat => Self::extract_anthropic_format(&json, provider),
            ModelExtractor::OllamaFormat => Self::extract_ollama_format(&json, provider),
        }
    }

    fn extract_openai_format(
        json: &serde_json::Value,
        provider: &str,
        free_by_id_only: bool,
    ) -> Result<Vec<DiscoveredModel>, ModelError> {
        let data = json.get("data").and_then(|d| d.as_array()).ok_or_else(|| {
            ModelError::Provider(format!("Unexpected response format from {}", provider))
        })?;

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
                .and_then(|v| {
                    v.as_u64()
                        .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()))
                })
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

            // Free-shaped ids are the OpenRouter/CommandCode convention:
            // "…:free", "…-free", and transient "stealth/…" previews.
            let free_by_id =
                id.ends_with(":free") || id.ends_with("-free") || id.starts_with("stealth/");
            let is_free = if free_by_id_only {
                free_by_id
            } else {
                (input_cost == 0.0 && output_cost == 0.0) || free_by_id
            };

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
                reasoning: item
                    .get("reasoning")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                is_free,
            });
        }

        // Sort: free models first, then by name
        models.sort_by(|a, b| b.is_free.cmp(&a.is_free).then_with(|| a.name.cmp(&b.name)));

        Ok(models)
    }

    fn extract_anthropic_format(
        json: &serde_json::Value,
        provider: &str,
    ) -> Result<Vec<DiscoveredModel>, ModelError> {
        let data = json.get("data").and_then(|d| d.as_array()).ok_or_else(|| {
            ModelError::Provider(format!("Unexpected response format from {}", provider))
        })?;

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
                reasoning: true,
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
            .ok_or_else(|| {
                ModelError::Provider(format!("Unexpected response format from {}", provider))
            })?;

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
                reasoning: false,
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
        let opencode = endpoints
            .get("opencode")
            .expect("opencode endpoint must exist");
        assert_eq!(opencode.base_url, "https://opencode.ai/zen/v1");
        assert_eq!(opencode.path, "/models");

        // Go's chat base is `…/go/openai/v1`, but its model list is served from
        // `…/go/v1/models` (the chat-shaped path is a 404).
        let go = endpoints
            .get("opencode-go")
            .expect("opencode-go endpoint must exist");
        assert_eq!(go.base_url, "https://opencode.ai/inference/go/v1");
        assert_eq!(go.path, "/models");
        assert!(go.free_by_id_only);

        let cline = endpoints.get("cline").expect("cline endpoint must exist");
        assert_eq!(cline.base_url, "https://api.cline.bot/api/v1");
        assert_eq!(cline.path, "/models");

        let tr = endpoints
            .get("tokenrouter")
            .expect("tokenrouter endpoint must exist");
        assert_eq!(tr.base_url, "https://api.tokenrouter.com/v1");
        assert_eq!(tr.path, "/models");

        let cc = endpoints
            .get("commandcode")
            .expect("commandcode endpoint must exist");
        assert_eq!(cc.base_url, "https://api.commandcode.ai/provider/v1");
        assert_eq!(cc.path, "/models");
        assert_eq!(cc.auth_style, AuthStyle::BearerOptional);
        assert!(
            cc.free_by_id_only,
            "commandcode omits pricing — id decides free"
        );
    }

    #[test]
    fn test_extract_openai_format_commandcode_free_ids() {
        let json = serde_json::json!({
            "object": "list",
            "data": [
                { "id": "claude-sonnet-5", "name": "Claude Sonnet 5", "context_length": 1000000, "supported_endpoints": ["/messages"] },
                { "id": "poolside/laguna-s-2.1-free", "name": "Laguna S 2.1" },
                { "id": "inclusionai/ling-3.0-flash-sante:free", "name": "Ling 3.0 Flash Sante" },
                { "id": "stealth/pixel-canary", "name": "Pixel Canary" }
            ]
        });

        let models = ModelDiscovery::extract_openai_format(&json, "commandcode", true)
            .expect("must extract");
        let by_id: std::collections::HashMap<_, _> =
            models.iter().map(|m| (m.id.as_str(), m.is_free)).collect();
        assert!(!by_id["claude-sonnet-5"], "no pricing must not imply free");
        assert!(by_id["poolside/laguna-s-2.1-free"]);
        assert!(by_id["inclusionai/ling-3.0-flash-sante:free"]);
        assert!(by_id["stealth/pixel-canary"]);
        // Free models sort first.
        assert!(models.iter().take(3).all(|m| m.is_free));
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

        let models = ModelDiscovery::extract_openai_format(&json, "opencode", false)
            .expect("must extract models");
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "claude-sonnet-4-5");
        assert_eq!(models[1].id, "gpt-5");
    }

    #[test]
    fn go_metadata_is_read_from_models_dev_shape() {
        let json = serde_json::json!({
            "opencode-go": {
                "id": "opencode-go",
                "models": {
                    "deepseek-v4.1-flash": {
                        "id": "deepseek-v4.1-flash",
                        "name": "DeepSeek V4.1 Flash",
                        "reasoning": true,
                        "tool_call": true,
                        "attachment": false,
                        "provider": { "npm": "@ai-sdk/openai-compatible" },
                        "limit": { "context": 1000000, "output": 131072 },
                        "cost": { "input": 0.15, "output": 0.6 }
                    },
                    "claude-haiku-5-5": {
                        "id": "claude-haiku-5-5",
                        "name": "Claude Haiku 5.5",
                        "provider": { "npm": "@ai-sdk/anthropic" },
                        "cost": { "input": 0.1, "output": 0.5 }
                    },
                    "gpt-6-luna": {
                        "id": "gpt-6-luna",
                        "name": "GPT 6 Luna",
                        "provider": { "npm": "@ai-sdk/openai" },
                        "cost": { "input": 0.1, "output": 0.5 }
                    },
                    "gemini-4-argon": {
                        "id": "gemini-4-argon",
                        "name": "Gemini 4 Argon",
                        "provider": { "npm": "@ai-sdk/google" }
                    }
                }
            }
        });

        let meta = parse_go_meta(&json);
        assert_eq!(meta.len(), 4);
        assert_eq!(meta["deepseek-v4.1-flash"].protocol, "openai");
        assert_eq!(meta["deepseek-v4.1-flash"].context_window, 1_000_000);
        assert_eq!(meta["deepseek-v4.1-flash"].input_cost_micros, 150_000);
        assert!(meta["deepseek-v4.1-flash"].reasoning);
        assert_eq!(meta["claude-haiku-5-5"].protocol, "anthropic");
        assert_eq!(meta["gpt-6-luna"].protocol, "responses");
        assert_eq!(meta["gemini-4-argon"].protocol, "google");
    }

    #[test]
    fn go_discovery_keeps_supported_protocols_and_drops_the_rest() {
        let live = vec![
            DiscoveredModel {
                id: "deepseek-v4.1-flash".into(),
                name: "deepseek-v4.1-flash".into(),
                context_window: 8192,
                input_cost_per_1m: 0.0,
                output_cost_per_1m: 0.0,
                supports_vision: false,
                supports_tools: false,
                reasoning: false,
                is_free: false,
            },
            DiscoveredModel {
                id: "claude-haiku-5-5".into(),
                name: "claude-haiku-5-5".into(),
                context_window: 8192,
                input_cost_per_1m: 0.0,
                output_cost_per_1m: 0.0,
                supports_vision: false,
                supports_tools: false,
                reasoning: false,
                is_free: false,
            },
            DiscoveredModel {
                id: "gpt-6-luna".into(),
                name: "gpt-6-luna".into(),
                context_window: 8192,
                input_cost_per_1m: 0.0,
                output_cost_per_1m: 0.0,
                supports_vision: false,
                supports_tools: false,
                reasoning: false,
                is_free: false,
            },
            DiscoveredModel {
                id: "gemini-4-argon".into(),
                name: "gemini-4-argon".into(),
                context_window: 8192,
                input_cost_per_1m: 0.0,
                output_cost_per_1m: 0.0,
                supports_vision: false,
                supports_tools: false,
                reasoning: false,
                is_free: false,
            },
            // Newer than models.dev: no exact entry, but its family is
            // Responses-only, so it must be dropped.
            DiscoveredModel {
                id: "grok-4.6".into(),
                name: "grok-4.6".into(),
                context_window: 8192,
                input_cost_per_1m: 0.0,
                output_cost_per_1m: 0.0,
                supports_vision: false,
                supports_tools: true,
                reasoning: false,
                is_free: false,
            },
            // Also newer than models.dev, but its family is OpenAI-compatible.
            DiscoveredModel {
                id: "glm-5.1".into(),
                name: "glm-5.1".into(),
                context_window: 8192,
                input_cost_per_1m: 0.0,
                output_cost_per_1m: 0.0,
                supports_vision: false,
                supports_tools: true,
                reasoning: false,
                is_free: false,
            },
        ];
        let mut meta = HashMap::new();
        meta.insert(
            "deepseek-v4.1-flash".into(),
            GoModelMeta {
                name: "DeepSeek V4.1 Flash".into(),
                context_window: 1_000_000,
                input_cost_micros: 300_000,
                output_cost_micros: 1_200_000,
                supports_tools: true,
                reasoning: true,
                protocol: "openai".into(),
                ..Default::default()
            },
        );
        meta.insert(
            "claude-haiku-5-5".into(),
            GoModelMeta {
                name: "Claude Haiku 5.5".into(),
                protocol: "anthropic".into(),
                supports_tools: true,
                ..Default::default()
            },
        );
        meta.insert(
            "gpt-6-luna".into(),
            GoModelMeta {
                protocol: "responses".into(),
                ..Default::default()
            },
        );
        meta.insert(
            "gemini-4-argon".into(),
            GoModelMeta {
                protocol: "google".into(),
                ..Default::default()
            },
        );
        meta.insert(
            "grok-4.7".into(),
            GoModelMeta {
                protocol: "responses".into(),
                ..Default::default()
            },
        );
        meta.insert(
            "glm-5.3".into(),
            GoModelMeta {
                protocol: "openai".into(),
                ..Default::default()
            },
        );

        let out = apply_go_meta(live, &meta);
        let ids: Vec<&str> = out.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&"deepseek-v4.1-flash"));
        assert!(ids.contains(&"claude-haiku-5-5"));
        assert!(!ids.contains(&"gpt-6-luna"), "responses is not implemented");
        assert!(
            !ids.contains(&"gemini-4-argon"),
            "google is not implemented"
        );
        assert!(
            !ids.contains(&"grok-4.6"),
            "a new Grok inherits its family's protocol"
        );
        assert!(
            ids.contains(&"glm-5.1"),
            "a new GLM inherits the OpenAI-compatible family"
        );

        let ds = out.iter().find(|m| m.id == "deepseek-v4.1-flash").unwrap();
        assert_eq!(ds.name, "DeepSeek V4.1 Flash");
        assert_eq!(ds.context_window, 1_000_000);
        assert_eq!(ds.input_cost_per_1m, 0.3);
        assert_eq!(ds.output_cost_per_1m, 1.2);
        assert!(ds.supports_tools);
        assert!(ds.reasoning);
        assert!(!ds.is_free);
    }

    #[test]
    fn go_metadata_unavailable_does_not_empty_the_list() {
        let live = vec![DiscoveredModel {
            id: "deepseek-v4.1-flash".into(),
            name: "deepseek-v4.1-flash".into(),
            context_window: 8192,
            input_cost_per_1m: 0.0,
            output_cost_per_1m: 0.0,
            supports_vision: false,
            supports_tools: true,
            reasoning: false,
            is_free: false,
        }];
        let out = apply_go_meta(live, &HashMap::new());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, "deepseek-v4.1-flash");
    }

    /// Live end-to-end check of the self-updating Go catalog.
    ///
    /// Ignored by default because it needs the network and a real key. Run:
    /// `OPENCODE_GO_API_KEY=… cargo test -p ravenbot-models -- --ignored live_opencode_go_discovery`
    #[tokio::test]
    #[ignore = "network + real OpenCode Go key"]
    async fn live_opencode_go_discovery_enriches_from_models_dev() {
        let key =
            std::env::var("OPENCODE_GO_API_KEY").expect("set OPENCODE_GO_API_KEY to run this test");
        let discovery = ModelDiscovery::new(Duration::from_secs(1));
        let models = discovery
            .fetch_models("opencode-go", Some(&key), true)
            .await
            .expect("discovery must succeed");

        assert!(!models.is_empty(), "the live Go list must not be empty");
        assert!(
            models.iter().any(|m| m.id == "deepseek-v4.1-flash"),
            "a known chat-completions model must survive filtering"
        );
        assert!(
            models.iter().any(|m| m.id.starts_with("claude")),
            "Claude models are kept and routed through the Anthropic provider"
        );
        assert!(
            !models.iter().any(|m| m.id == "gpt-6-luna"),
            "Responses-only models are filtered until an adapter exists"
        );

        let ds = models
            .iter()
            .find(|m| m.id == "deepseek-v4.1-flash")
            .expect("deepseek-v4.1-flash");
        assert!(
            ds.context_window > 100_000,
            "models.dev metadata must replace the 8192 fallback (got {})",
            ds.context_window
        );
        assert!(ds.supports_tools);
        assert!(ds.input_cost_per_1m > 0.0, "pricing must be enriched");
    }
}

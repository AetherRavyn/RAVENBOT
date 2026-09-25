//! Generic OpenAI-compatible provider.
//!
//! Used by DeepSeek, Groq, Together, Perplexity, Mistral, Gemini (OpenAI variant),
//! and any future provider that speaks the OpenAI Chat Completions wire format.

use async_trait::async_trait;
use ravenbot_core::ModelProvider;
use serde::{Deserialize, Serialize};

use super::{ModelProviderTrait, ModelResponse, Message, ToolDefinition, ModelError, Usage, DeltaCallback, StreamAccumulator, streaming};

pub struct OpenAiCompatibleProvider {
    api_key: Option<String>,
    model_id: String,
    base_url: String,
    provider: ModelProvider,
    referer: Option<String>,
}

impl OpenAiCompatibleProvider {
    pub fn new(
        api_key: Option<String>,
        base_url: impl Into<String>,
        default_model: impl Into<String>,
        provider: ModelProvider,
    ) -> Self {
        Self {
            api_key,
            model_id: default_model.into(),
            base_url: base_url.into(),
            provider,
            referer: None,
        }
    }

    pub fn with_referer(mut self, referer: impl Into<String>) -> Self {
        self.referer = Some(referer.into());
        self
    }

    pub fn with_model(mut self, model_id: impl Into<String>) -> Self {
        self.model_id = model_id.into();
        self
    }

    async fn send_chat(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        temperature: f32,
        max_tokens: u32,
        stream: bool,
        on_delta: Option<&DeltaCallback>,
        _enable_reasoning: bool,
    ) -> Result<ModelResponse, ModelError> {
        // Shared OpenAI-compat path (DeepSeek/Groq/Gemini/Mistral/Together/
        // Perplexity/Cohere): clamp to 0..=2 so strict gateways 400 less.
        let temperature = temperature.clamp(0.0, 2.0);
        let api_key = self.api_key.as_ref()
            .ok_or_else(|| ModelError::Auth(format!("{} API key not configured", self.provider.display_name())))?;

        let chat_messages: Vec<ChatMessage> = messages.iter().map(|m| {
            ChatMessage {
                role: m.role.clone(),
                content: m.openai_content(),
                tool_calls: m.openai_tool_calls(),
                tool_call_id: m.tool_call_id.clone(),
                name: m.name.clone(),
            }
        }).collect();

        let tools_param = if tools.is_empty() {
            None
        } else {
            Some(tools.iter().map(|t| ToolParam {
                tool_type: "function".to_string(),
                function: FunctionParam {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.parameters.clone(),
                },
            }).collect())
        };

        let request = ChatRequest {
            model: self.model_id.clone(),
            messages: chat_messages,
            tools: tools_param,
            temperature,
            max_tokens,
            stream,
            stream_options: if stream {
                Some(StreamOptions { include_usage: true })
            } else {
                None
            },
        };

        let mut req_builder = self.client()
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&request);

        if let Some(ref r) = self.referer {
            req_builder = req_builder.header("HTTP-Referer", r);
        }

        let response = req_builder.send().await.map_err(ModelError::Http)?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ModelError::Provider(format!("API error {}: {}", status, body)));
        }

        if let Some(on_delta) = on_delta {
            let mut acc = StreamAccumulator::new();
            streaming::consume_sse(response, |json| {
                if let Some(usage) = json.get("usage") {
                    let input = usage.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                    let output = usage.get("completion_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                    if input > 0 || output > 0 {
                        acc.set_usage(input, output);
                    }
                }
                if let Some(choice) = json.get("choices").and_then(|c| c.get(0)) {
                    if let Some(delta) = choice.get("delta") {
                        if let Some(text) = delta.get("content").and_then(|v| v.as_str()) {
                            if !text.is_empty() {
                                acc.push_text(text);
                                on_delta(text);
                            }
                        }
                        if let Some(tc) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                            for chunk in tc {
                                let func = chunk.get("function");
                                acc.push_tool_call_delta(
                                    chunk.get("index").and_then(|v| v.as_u64()).map(|v| v as usize),
                                    chunk.get("id").and_then(|v| v.as_str()),
                                    func.and_then(|f| f.get("name")).and_then(|v| v.as_str()),
                                    func.and_then(|f| f.get("arguments")).and_then(|v| v.as_str()),
                                );
                            }
                        }
                    }
                }
                Ok(())
            }).await?;
            Ok(acc.finish())
        } else {
            let chat_response: ChatResponse = response.json().await.map_err(ModelError::Http)?;
            let choice = chat_response.choices.first()
                .ok_or_else(|| ModelError::Provider("No response choices".to_string()))?;

            let tool_calls = choice.message.tool_calls.as_ref()
                .unwrap_or(&vec![])
                .iter()
                .filter_map(|tc| {
                    let args = serde_json::from_str(&tc.function.arguments).ok()?;
                    Some(super::ToolCall {
                        name: tc.function.name.clone(),
                        arguments: args,
                        id: tc.id.clone(),
                    })
                })
                .collect();

            let usage = chat_response.usage.as_ref().map(|u| Usage {
                input_tokens: u.prompt_tokens,
                output_tokens: u.completion_tokens,
            }).unwrap_or(Usage { input_tokens: 0, output_tokens: 0 });

            Ok(ModelResponse {
                content: choice.message.content.clone(),
                tool_calls,
                usage,
                reasoning: None,
            })
        }
    }

    fn client(&self) -> reqwest::Client {
        reqwest::Client::new()
    }
}

#[async_trait]
impl ModelProviderTrait for OpenAiCompatibleProvider {
    fn provider_type(&self) -> ModelProvider { self.provider.clone() }

    async fn complete(&self, messages: &[Message], tools: &[ToolDefinition], temperature: f32, max_tokens: u32) -> Result<ModelResponse, ModelError> {
        self.send_chat(messages, tools, temperature, max_tokens, false, None, false).await
    }

    async fn complete_stream(&self, messages: &[Message], tools: &[ToolDefinition], temperature: f32, max_tokens: u32, on_delta: DeltaCallback, enable_reasoning: bool) -> Result<ModelResponse, ModelError> {
        self.send_chat(messages, tools, temperature, max_tokens, true, Some(&on_delta), enable_reasoning).await
    }

    fn with_model(mut self: Box<Self>, model_id: String) -> Box<dyn ModelProviderTrait> {
        self.model_id = model_id;
        Box::new(*self)
    }

    async fn health_check(&self) -> Result<bool, ModelError> {
        let api_key = match &self.api_key {
            Some(key) => key,
            None => return Ok(false),
        };
        let response = self.client()
            .get(format!("{}/models", self.base_url))
            .header("Authorization", format!("Bearer {}", api_key))
            .send()
            .await;
        Ok(response.is_ok())
    }
}

// ── Shared wire types ─────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ToolParam>>,
    temperature: f32,
    max_tokens: u32,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
}

#[derive(Debug, Serialize)]
struct StreamOptions { include_usage: bool }

#[derive(Debug, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<super::WireToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

#[derive(Debug, Serialize)]
struct ToolParam {
    #[serde(rename = "type")]
    tool_type: String,
    function: FunctionParam,
}

#[derive(Debug, Serialize)]
struct FunctionParam {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
    usage: Option<UsageResponse>,
}

#[derive(Debug, Deserialize)]
struct Choice { message: ResponseMessage }

#[derive(Debug, Deserialize)]
struct ResponseMessage {
    content: Option<String>,
    tool_calls: Option<Vec<ToolCallResponse>>,
}

#[derive(Debug, Deserialize)]
struct ToolCallResponse {
    id: String,
    function: FunctionCall,
}

#[derive(Debug, Deserialize)]
struct FunctionCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct UsageResponse {
    prompt_tokens: u64,
    completion_tokens: u64,
}

// ── Public constructors for each provider ─────────────────────────────────────

pub fn deepseek_provider(api_key: Option<String>) -> OpenAiCompatibleProvider {
    OpenAiCompatibleProvider::new(api_key, "https://api.deepseek.com/v1", "deepseek-chat", ModelProvider::DeepSeek)
}

pub fn groq_provider(api_key: Option<String>) -> OpenAiCompatibleProvider {
    OpenAiCompatibleProvider::new(api_key, "https://api.groq.com/openai/v1", "llama-3.3-70b-versatile", ModelProvider::Groq)
}

pub fn gemini_provider(api_key: Option<String>) -> OpenAiCompatibleProvider {
    OpenAiCompatibleProvider::new(api_key, "https://generativelanguage.googleapis.com/v1beta/openai", "gemini-2.0-flash", ModelProvider::Gemini)
}

pub fn mistral_provider(api_key: Option<String>) -> OpenAiCompatibleProvider {
    OpenAiCompatibleProvider::new(api_key, "https://api.mistral.ai/v1", "mistral-large-latest", ModelProvider::Mistral)
}

pub fn together_provider(api_key: Option<String>) -> OpenAiCompatibleProvider {
    OpenAiCompatibleProvider::new(api_key, "https://api.together.xyz/v1", "meta-llama/Llama-3.3-70B-Instruct-Turbo", ModelProvider::Together)
}

pub fn perplexity_provider(api_key: Option<String>) -> OpenAiCompatibleProvider {
    OpenAiCompatibleProvider::new(api_key, "https://api.perplexity.ai", "llama-3.1-sonar-large-128k-online", ModelProvider::Perplexity)
}

pub fn cohere_provider(api_key: Option<String>) -> OpenAiCompatibleProvider {
    // Cohere's OpenAI-compatible surface lives under /compatibility/v1, so
    // `{base}/chat/completions` resolves correctly (the old /v2/chat base
    // produced /v2/chat/chat/completions → 404 on every call).
    OpenAiCompatibleProvider::new(api_key, "https://api.cohere.com/compatibility/v1", "command-r-plus", ModelProvider::Cohere)
}

pub fn opencode_provider(api_key: Option<String>) -> OpenAiCompatibleProvider {
    OpenAiCompatibleProvider::new(api_key, "https://opencode.ai/zen/v1", "claude-sonnet-4-5", ModelProvider::OpenCode)
}

pub fn cline_provider(api_key: Option<String>) -> OpenAiCompatibleProvider {
    OpenAiCompatibleProvider::new(api_key, "https://api.cline.bot/api/v1", "claude-sonnet-4-5", ModelProvider::Cline)
}

pub fn xai_provider(api_key: Option<String>) -> OpenAiCompatibleProvider {
    OpenAiCompatibleProvider::new(api_key, "https://api.x.ai/v1", "grok-2-latest", ModelProvider::XAI)
}

pub fn tokenrouter_provider(api_key: Option<String>) -> OpenAiCompatibleProvider {
    // Unified multi-model gateway: one OpenAI-compatible endpoint fronting
    // DeepSeek/Qwen/GLM/Kimi/GPT/Grok and more.
    OpenAiCompatibleProvider::new(
        api_key,
        "https://api.tokenrouter.com/v1",
        "deepseek/deepseek-v4.1-flash",
        ModelProvider::TokenRouter,
    )
}

/// User-defined provider on an arbitrary OpenAI-compatible base URL.
pub fn custom_provider(
    api_key: Option<String>,
    base_url: impl Into<String>,
    default_model: impl Into<String>,
) -> OpenAiCompatibleProvider {
    OpenAiCompatibleProvider::new(api_key, base_url, default_model, ModelProvider::Custom)
}

/// Built-in OpenAI-compatible provider re-pointed at a user-supplied base URL
/// (proxy/gateway override). Keeps the built-in's default model.
pub fn openai_compat_with_base(
    api_key: Option<String>,
    base_url: impl Into<String>,
    provider: ModelProvider,
) -> OpenAiCompatibleProvider {
    let default_model = match provider {
        ModelProvider::OpenRouter => "anthropic/claude-3.5-sonnet",
        ModelProvider::OpenAI => "gpt-4o",
        ModelProvider::DeepSeek => "deepseek-chat",
        ModelProvider::Groq => "llama-3.3-70b-versatile",
        ModelProvider::Gemini => "gemini-2.0-flash",
        ModelProvider::Mistral => "mistral-large-latest",
        ModelProvider::Together => "meta-llama/Llama-3.3-70B-Instruct-Turbo",
        ModelProvider::Perplexity => "llama-3.1-sonar-large-128k-online",
        ModelProvider::Cohere => "command-r-plus",
        ModelProvider::MiMo => "mimo-7b",
        ModelProvider::OpenCode => "claude-sonnet-4-5",
        ModelProvider::Cline => "claude-sonnet-4-5",
        ModelProvider::XAI => "grok-2-latest",
        ModelProvider::TokenRouter => "deepseek/deepseek-v4.1-flash",
        _ => "",
    };
    OpenAiCompatibleProvider::new(api_key, base_url, default_model, provider)
}


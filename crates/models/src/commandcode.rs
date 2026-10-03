//! Command Code provider — OpenAI/Anthropic compatible proxy.
//!
//! Supports both:
//! - /v1/chat/completions (OpenAI format)
//! - /v1/messages (Anthropic format)
//!
//! API docs: https://commandcode.ai/docs/api

use async_trait::async_trait;
use ravenbot_core::ModelProvider;
use serde::{Deserialize, Serialize};

use super::{StreamChunk,ModelProviderTrait, ModelResponse, Message, ToolDefinition, ModelError, Usage, DeltaCallback, StreamAccumulator, streaming};

const BASE_URL: &str = "https://api.commandcode.ai/provider/v1";

/// Zero-data-retention opt-in — mirrors the CLI's `CMD_ZDR=1`. Requests then
/// route only through ZDR-capable upstreams (422 if none exists).
fn zdr_enabled() -> bool {
    std::env::var("CMD_ZDR").map(|v| v.trim() == "1").unwrap_or(false)
}

pub struct CommandCodeProvider {
    api_key: Option<String>,
    model_id: String,
    client: reqwest::Client,
    use_anthropic_format: bool,
}

impl CommandCodeProvider {
    pub fn new(api_key: Option<String>) -> Self {
        Self {
            api_key,
            model_id: "deepseek/deepseek-v4-flash".to_string(),
            client: reqwest::Client::new(),
            use_anthropic_format: false,
        }
    }

    pub fn with_model(mut self, model_id: impl Into<String>) -> Self {
        let mid = model_id.into();
        // Auto-detect: Anthropic models answer on /messages only (per the
        // supported_endpoints field of /provider/v1/models).
        self.use_anthropic_format = mid.starts_with("claude-") || mid.starts_with("anthropic/");
        self.model_id = mid;
        self
    }

    /// Force Anthropic Messages format
    pub fn anthropic_format(mut self) -> Self {
        self.use_anthropic_format = true;
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
        enable_reasoning: bool,
    ) -> Result<ModelResponse, ModelError> {
        let api_key = self.api_key.as_ref()
            .ok_or_else(|| ModelError::Auth("Command Code API key not configured".to_string()))?;

        // CommandCode's gateway validates temperature <= 1 (upstream 400
        // otherwise) — clamp once so both wire formats are covered.
        let temperature = temperature.clamp(0.0, 1.0);

        if self.use_anthropic_format {
            self.send_anthropic(messages, tools, temperature, max_tokens, stream, on_delta, enable_reasoning, api_key).await
        } else {
            self.send_openai(messages, tools, temperature, max_tokens, stream, on_delta, api_key).await
        }
    }

    async fn send_openai(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        temperature: f32,
        max_tokens: u32,
        stream: bool,
        on_delta: Option<&DeltaCallback>,
        api_key: &str,
    ) -> Result<ModelResponse, ModelError> {
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

        #[derive(Serialize)]
        struct Req {
            model: String,
            messages: Vec<ChatMessage>,
            #[serde(skip_serializing_if = "Option::is_none")]
            tools: Option<Vec<ToolParam>>,
            temperature: f32,
            max_tokens: u32,
            stream: bool,
            #[serde(skip_serializing_if = "Option::is_none")]
            stream_options: Option<StreamOpts>,
        }
        #[derive(Serialize)]
        struct StreamOpts { include_usage: bool }

        let request = Req {
            model: self.model_id.clone(),
            messages: chat_messages,
            tools: tools_param,
            temperature,
            max_tokens,
            stream,
            stream_options: if stream { Some(StreamOpts { include_usage: true }) } else { None },
        };

        let mut builder = self.client
            .post(format!("{}/chat/completions", BASE_URL))
            .header("Authorization", format!("Bearer {}", api_key));
        if zdr_enabled() {
            builder = builder.header("x-cmd-zdr", "1");
        }
        let response = builder
            .json(&request)
            .send()
            .await
            .map_err(ModelError::Http)?;

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
                    if input > 0 || output > 0 { acc.set_usage(input, output); }
                }
                if let Some(choice) = json.get("choices").and_then(|c| c.get(0)) {
                    if let Some(delta) = choice.get("delta") {
                        if let Some(text) = delta.get("content").and_then(|v| v.as_str()) {
                            if !text.is_empty() { acc.push_text(text); on_delta(StreamChunk::Text(text)); }
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
            #[derive(Deserialize)]
            struct Resp { choices: Vec<Choice>, usage: Option<UsageResp> }
            #[derive(Deserialize)]
            struct Choice { message: RespMsg }
            #[derive(Deserialize)]
            struct RespMsg { content: Option<String>, tool_calls: Option<Vec<ToolCallResp>> }
            #[derive(Deserialize)]
            struct ToolCallResp { id: String, function: FuncCall }
            #[derive(Deserialize)]
            struct FuncCall { name: String, arguments: String }
            #[derive(Deserialize)]
            struct UsageResp { prompt_tokens: u64, completion_tokens: u64 }

            let resp: Resp = response.json().await.map_err(ModelError::Http)?;
            let choice = resp.choices.first().ok_or_else(|| ModelError::Provider("No choices".into()))?;
            let tool_calls = choice.message.tool_calls.as_ref().unwrap_or(&vec![]).iter().filter_map(|tc| {
                let args = serde_json::from_str(&tc.function.arguments).ok()?;
                Some(super::ToolCall { name: tc.function.name.clone(), arguments: args, id: tc.id.clone() })
            }).collect();
            let usage = resp.usage.as_ref().map(|u| Usage { input_tokens: u.prompt_tokens, output_tokens: u.completion_tokens })
                .unwrap_or(Usage { input_tokens: 0, output_tokens: 0 });
            Ok(ModelResponse { content: choice.message.content.clone(), tool_calls, usage, reasoning: None })
        }
    }

    async fn send_anthropic(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        temperature: f32,
        max_tokens: u32,
        stream: bool,
        on_delta: Option<&DeltaCallback>,
        enable_reasoning: bool,
        api_key: &str,
    ) -> Result<ModelResponse, ModelError> {
        // Extract system message
        let system_msg = messages.iter().find(|m| m.role == "system").map(|m| m.content.clone());
        let user_messages: Vec<&Message> = messages.iter().filter(|m| m.role != "system").collect();

        let anthro_messages: Vec<AnthroMsg> = user_messages.iter().map(|m| {
            let content = if m.images.is_empty() {
                serde_json::json!(m.content)
            } else {
                let mut parts = vec![serde_json::json!({"type": "text", "text": m.content})];
                for img in &m.images {
                    parts.push(serde_json::json!({
                        "type": "image",
                        "source": { "type": "base64", "media_type": img.mime, "data": img.data }
                    }));
                }
                serde_json::Value::Array(parts)
            };
            AnthroMsg { role: m.role.clone(), content }
        }).collect();

        let tools_param = if tools.is_empty() {
            None
        } else {
            Some(tools.iter().map(|t| AnthroTool {
                name: t.name.clone(),
                description: t.description.clone(),
                input_schema: t.parameters.clone(),
            }).collect())
        };

        #[derive(Serialize)]
        struct Req {
            model: String,
            messages: Vec<AnthroMsg>,
            max_tokens: u32,
            #[serde(skip_serializing_if = "Option::is_none")]
            system: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            tools: Option<Vec<AnthroTool>>,
            #[serde(skip_serializing_if = "Option::is_none")]
            temperature: Option<f32>,
            stream: bool,
            #[serde(skip_serializing_if = "Option::is_none")]
            thinking: Option<ThinkingCfg>,
        }
        #[derive(Serialize)]
        struct ThinkingCfg { #[serde(rename = "type")] kind: String, budget_tokens: u32 }

        let request = Req {
            model: self.model_id.clone(),
            messages: anthro_messages,
            max_tokens,
            system: system_msg,
            tools: tools_param,
            temperature: if enable_reasoning { None } else { Some(temperature) },
            stream,
            thinking: if enable_reasoning { Some(ThinkingCfg { kind: "enabled".into(), budget_tokens: (max_tokens as f64 * 0.6) as u32 }) } else { None },
        };

        let mut builder = self.client
            .post(format!("{}/messages", BASE_URL))
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json");
        if zdr_enabled() {
            builder = builder.header("x-cmd-zdr", "1");
        }
        let response = builder
            .json(&request)
            .send()
            .await
            .map_err(ModelError::Http)?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ModelError::Provider(format!("API error {}: {}", status, body)));
        }

        if let Some(on_delta) = on_delta {
            let mut acc = StreamAccumulator::new();
            streaming::consume_sse(response, |json| {
                match json.get("type").and_then(|v| v.as_str()) {
                    Some("message_start") => {
                        if let Some(usage) = json.get("message").and_then(|m| m.get("usage")) {
                            let input = usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                            if input > 0 { acc.set_usage(input, 0); }
                        }
                    }
                    Some("content_block_start") => {
                        if let Some(block) = json.get("content_block") {
                            if block.get("type").and_then(|v| v.as_str()) == Some("tool_use") {
                                let idx = json.get("index").and_then(|v| v.as_u64()).map(|v| v as usize).unwrap_or(0);
                                if let (Some(id), Some(name)) = (
                                    block.get("id").and_then(|v| v.as_str()),
                                    block.get("name").and_then(|v| v.as_str()),
                                ) {
                                    acc.push_tool_use_start(idx, id, name);
                                }
                            }
                        }
                    }
                    Some("content_block_delta") => {
                        if let Some(delta) = json.get("delta") {
                            match delta.get("type").and_then(|v| v.as_str()) {
                                Some("text_delta") => {
                                    if let Some(text) = delta.get("text").and_then(|v| v.as_str()) {
                                        if !text.is_empty() { acc.push_text(text); on_delta(StreamChunk::Text(text)); }
                                    }
                                }
                                Some("thinking_delta") => {
                                    if let Some(text) = delta.get("thinking").and_then(|v| v.as_str()) {
                                        acc.push_reasoning(text);
                                    }
                                }
                                Some("input_json_delta") => {
                                    if let Some(partial) = delta.get("partial_json").and_then(|v| v.as_str()) {
                                        let idx = json.get("index").and_then(|v| v.as_u64()).map(|v| v as usize).unwrap_or(0);
                                        acc.push_tool_json_delta(idx, partial);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    Some("message_delta") => {
                        if let Some(usage) = json.get("usage") {
                            let output = usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                            if output > 0 {
                                acc.set_usage(0, output);
                            }
                        }
                    }
                    _ => {}
                }
                Ok(())
            }).await?;
            Ok(acc.finish())
        } else {
            #[derive(Deserialize)]
            struct Resp { content: Vec<ContentBlock>, usage: AnthroUsage }
            #[derive(Deserialize)]
            struct ContentBlock { #[serde(rename = "type")] kind: String, text: Option<String>, name: Option<String>, id: Option<String>, input: Option<serde_json::Value> }
            #[derive(Deserialize)]
            struct AnthroUsage { input_tokens: u64, output_tokens: u64 }

            let resp: Resp = response.json().await.map_err(ModelError::Http)?;
            let content_text: String = resp.content.iter()
                .filter(|b| b.kind == "text")
                .filter_map(|b| b.text.clone())
                .collect::<Vec<_>>()
                .join("\n");
            let tool_calls: Vec<super::ToolCall> = resp.content.iter()
                .filter(|b| b.kind == "tool_use")
                .filter_map(|b| {
                    Some(super::ToolCall {
                        name: b.name.clone()?,
                        arguments: b.input.clone()?,
                        id: b.id.clone()?,
                    })
                })
                .collect();
            Ok(ModelResponse {
                content: if content_text.is_empty() { None } else { Some(content_text) },
                tool_calls,
                usage: Usage { input_tokens: resp.usage.input_tokens, output_tokens: resp.usage.output_tokens },
                reasoning: None,
            })
        }
    }
}

#[derive(Debug, Serialize)]
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
    #[serde(rename = "type")] tool_type: String,
    function: FunctionParam,
}

#[derive(Debug, Serialize)]
struct FunctionParam { name: String, description: String, parameters: serde_json::Value }

#[derive(Debug, Serialize)]
struct AnthroMsg { role: String, content: serde_json::Value }

#[derive(Debug, Serialize)]
struct AnthroTool { name: String, description: String, input_schema: serde_json::Value }

#[async_trait]
impl ModelProviderTrait for CommandCodeProvider {
    fn provider_type(&self) -> ModelProvider { ModelProvider::CommandCode }

    async fn complete(&self, messages: &[Message], tools: &[ToolDefinition], temperature: f32, max_tokens: u32) -> Result<ModelResponse, ModelError> {
        self.send_chat(messages, tools, temperature, max_tokens, false, None, false).await
    }

    async fn complete_stream(&self, messages: &[Message], tools: &[ToolDefinition], temperature: f32, max_tokens: u32, on_delta: DeltaCallback, enable_reasoning: bool) -> Result<ModelResponse, ModelError> {
        self.send_chat(messages, tools, temperature, max_tokens, true, Some(&on_delta), enable_reasoning).await
    }

    fn with_model(self: Box<Self>, model_id: String) -> Box<dyn ModelProviderTrait> {
        // Route through the builder so the /messages-vs-/chat/completions
        // auto-detection is re-evaluated for the new model id.
        Box::new((*self).with_model(model_id))
    }

    async fn health_check(&self) -> Result<bool, ModelError> {
        let api_key = match &self.api_key {
            Some(key) => key,
            None => return Ok(false),
        };
        let response = self.client
            .get(format!("{}/models", BASE_URL))
            .header("Authorization", format!("Bearer {}", api_key))
            .send()
            .await;
        match response {
            Ok(r) => Ok(r.status().is_success()),
            Err(_) => Ok(false),
        }
    }
}

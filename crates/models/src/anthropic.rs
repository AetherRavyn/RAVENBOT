//! Anthropic model provider

use async_trait::async_trait;
use ravenbot_core::ModelProvider;
use serde::{Deserialize, Serialize};

use super::{ModelProviderTrait, ModelResponse, Message, ToolDefinition, ModelError, Usage, DeltaCallback, StreamAccumulator, streaming};

const BASE_URL: &str = "https://api.anthropic.com/v1";

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ToolParam>>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    system: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
    /// Extended thinking (visible chain-of-thought)
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<ThinkingConfig>,
}

#[derive(Debug, Serialize)]
struct ThinkingConfig {
    #[serde(rename = "type")]
    thinking_type: &'static str,
    budget_tokens: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct ToolParam {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    content: Vec<ContentBlock>,
    usage: UsageResponse,
    #[allow(dead_code)]
    stop_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse { id: String, name: String, input: serde_json::Value },
}

#[derive(Debug, Deserialize)]
struct UsageResponse {
    input_tokens: u64,
    output_tokens: u64,
}

pub struct AnthropicProvider {
    api_key: Option<String>,
    model_id: String,
    client: reqwest::Client,
}

impl AnthropicProvider {
    pub fn new(api_key: Option<String>) -> Self {
        Self {
            api_key,
            model_id: "claude-3-5-sonnet-20241022".to_string(),
            client: reqwest::Client::new(),
        }
    }

    pub fn with_model(mut self, model_id: impl Into<String>) -> Self {
        self.model_id = model_id.into();
        self
    }

    /// Shared send path for both non-streaming and streaming requests
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
            .ok_or_else(|| ModelError::Auth("Anthropic API key not configured".to_string()))?;

        // Extract system message if present
        let system_message = messages.iter()
            .find(|m| m.role == "system")
            .map(|m| m.content.clone());

        // Filter out system message from messages
        let chat_messages: Vec<ChatMessage> = messages.iter()
            .filter(|m| m.role != "system")
            .map(|m| {
                // Vision: image blocks precede the text block (Anthropic format)
                let content = if m.images.is_empty() {
                    serde_json::json!(m.content)
                } else {
                    let mut blocks = Vec::new();
                    for img in &m.images {
                        blocks.push(serde_json::json!({
                            "type": "image",
                            "source": {
                                "type": "base64",
                                "media_type": img.mime,
                                "data": img.data
                            }
                        }));
                    }
                    blocks.push(serde_json::json!({"type": "text", "text": m.content}));
                    serde_json::Value::Array(blocks)
                };
                ChatMessage { role: m.role.clone(), content }
            })
            .collect();

        let tools_param = if tools.is_empty() {
            None
        } else {
            Some(tools.iter().map(|t| ToolParam {
                name: t.name.clone(),
                description: t.description.clone(),
                input_schema: t.parameters.clone(),
            }).collect())
        };

        let request = ChatRequest {
            model: self.model_id.clone(),
            messages: chat_messages,
            tools: tools_param,
            max_tokens,
            // Anthropic constraint: temperature must be exactly 1 when
            // extended thinking is enabled
            temperature: if enable_reasoning { Some(1.0) } else { Some(temperature) },
            system: system_message,
            stream,
            thinking: if enable_reasoning {
                // Budget must be strictly less than max_tokens; keep ~60%
                let budget = std::cmp::max(1024, max_tokens * 60 / 100);
                Some(ThinkingConfig {
                    thinking_type: "enabled",
                    budget_tokens: budget.min(max_tokens.saturating_sub(1)),
                })
            } else {
                None
            },
        };

        let response = self.client
            .post(format!("{}/messages", BASE_URL))
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
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
            // Streaming path: Anthropic event-based SSE
            let mut acc = StreamAccumulator::new();
            // Extended thinking: reasoning deltas stream inside  swell tags so
            // the UI's Reasoning panel renders them live; they are kept out of
            // the final content (returned separately as `reasoning`).
            let mut thinking_stream_open = false;
            streaming::consume_sse(response, |json| {
                match json.get("type").and_then(|v| v.as_str()).unwrap_or("") {
                    "message_start" => {
                        if let Some(input) = json.pointer("/message/usage/input_tokens").and_then(|v| v.as_u64()) {
                            let output = acc.usage().map(|u| u.output_tokens).unwrap_or(0);
                            acc.set_usage(input, output);
                        }
                    }
                    "content_block_start" => {
                        if let Some(block) = json.get("content_block") {
                            if block.get("type").and_then(|v| v.as_str()) == Some("tool_use") {
                                let index = json.get("index").and_then(|v| v.as_u64()).map(|v| v as usize).unwrap_or(0);
                                let id = block.get("id").and_then(|v| v.as_str()).unwrap_or("");
                                let name = block.get("name").and_then(|v| v.as_str()).unwrap_or("");
                                acc.push_tool_use_start(index, id, name);
                            }
                        }
                    }
                    "content_block_delta" => {
                        let index = json.get("index").and_then(|v| v.as_u64()).map(|v| v as usize).unwrap_or(0);
                        if let Some(delta) = json.get("delta") {
                            match delta.get("type").and_then(|v| v.as_str()).unwrap_or("") {
                                "text_delta" => {
                                    if let Some(text) = delta.get("text").and_then(|v| v.as_str()) {
                                        if !text.is_empty() {
                                            if thinking_stream_open {
                                                on_delta("\n\n");
                                                thinking_stream_open = false;
                                            }
                                            acc.push_text(text);
                                            on_delta(text);
                                        }
                                    }
                                }
                                "thinking_delta" => {
                                    if let Some(thinking) = delta.get("thinking").and_then(|v| v.as_str()) {
                                        if !thinking.is_empty() {
                                            if !thinking_stream_open {
                                                on_delta("feel");
                                                thinking_stream_open = true;
                                            }
                                            acc.push_reasoning(thinking);
                                            on_delta(thinking);
                                        }
                                    }
                                }
                                // Signature blocks accompany thinking — never rendered
                                "signature_delta" => {}
                                "input_json_delta" => {
                                    if let Some(pj) = delta.get("partial_json").and_then(|v| v.as_str()) {
                                        acc.push_tool_json_delta(index, pj);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    "message_delta" => {
                        if let Some(output) = json.pointer("/usage/output_tokens").and_then(|v| v.as_u64()) {
                            let input = acc.usage().map(|u| u.input_tokens).unwrap_or(0);
                            acc.set_usage(input, output);
                        }
                    }
                    _ => {}
                }
                Ok(())
            }).await?;
            if thinking_stream_open {
                // Close an unterminated reasoning block (thinking-only responses)
                on_delta("\n\n");
            }

            Ok(acc.finish())
        } else {
            // Non-streaming path
            let chat_response: ChatResponse = response.json().await
                .map_err(ModelError::Http)?;

            let mut content_text = String::new();
            let mut tool_calls = Vec::new();

            for block in &chat_response.content {
                match block {
                    ContentBlock::Text { text } => {
                        content_text.push_str(text);
                    }
                    ContentBlock::ToolUse { id, name, input } => {
                        tool_calls.push(super::ToolCall {
                            name: name.clone(),
                            arguments: input.clone(),
                            id: id.clone(),
                        });
                    }
                }
            }

            let usage = Usage {
                input_tokens: chat_response.usage.input_tokens,
                output_tokens: chat_response.usage.output_tokens,
            };

            Ok(ModelResponse {
                content: if content_text.is_empty() { None } else { Some(content_text) },
                tool_calls,
                usage,
                reasoning: None,
            })
        }
    }
}

#[async_trait]
impl ModelProviderTrait for AnthropicProvider {
    fn provider_type(&self) -> ModelProvider {
        ModelProvider::Anthropic
    }

    async fn complete(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        temperature: f32,
        max_tokens: u32,
    ) -> Result<ModelResponse, ModelError> {
        self.send_chat(messages, tools, temperature, max_tokens, false, None, false).await
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

        let response = self.client
            .get(format!("{}/models", BASE_URL))
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .send()
            .await;

        Ok(response.is_ok())
    }
}

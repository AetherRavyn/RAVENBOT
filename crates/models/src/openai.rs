//! OpenAI model provider

use async_trait::async_trait;
use ravenbot_core::ModelProvider;
use serde::{Deserialize, Serialize};

use super::{ModelProviderTrait, ModelResponse, Message, ToolDefinition, ModelError, Usage, DeltaCallback, StreamAccumulator, streaming};

const BASE_URL: &str = "https://api.openai.com/v1";

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
struct StreamOptions {
    #[serde(rename = "include_usage")]
    include_usage: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: serde_json::Value,
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
struct Choice {
    message: ResponseMessage,
}

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

pub struct OpenAIProvider {
    api_key: Option<String>,
    model_id: String,
    client: reqwest::Client,
}

impl OpenAIProvider {
    pub fn new(api_key: Option<String>) -> Self {
        Self {
            api_key,
            model_id: "gpt-4o".to_string(),
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
            .ok_or_else(|| ModelError::Auth("OpenAI API key not configured".to_string()))?;

        let chat_messages: Vec<ChatMessage> = messages.iter().map(|m| {
            // Vision: text + inline image parts (data URI) when images present
            let content = if m.images.is_empty() {
                serde_json::json!(m.content)
            } else {
                let mut parts = vec![serde_json::json!({"type": "text", "text": m.content})];
                for img in &m.images {
                    parts.push(serde_json::json!({
                        "type": "image_url",
                        "image_url": { "url": format!("data:{};base64,{}", img.mime, img.data) }
                    }));
                }
                serde_json::Value::Array(parts)
            };
            ChatMessage { role: m.role.clone(), content }
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

        let response = self.client
            .post(format!("{}/chat/completions", BASE_URL))
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
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
            // Streaming path: parse SSE chunks, forward deltas live
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
            // Non-streaming path
            let chat_response: ChatResponse = response.json().await
                .map_err(ModelError::Http)?;

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
            }).unwrap_or(Usage {
                input_tokens: 0,
                output_tokens: 0,
            });

            Ok(ModelResponse {
                content: choice.message.content.clone(),
                tool_calls,
                usage,
                reasoning: None,
            })
        }
    }
}

#[async_trait]
impl ModelProviderTrait for OpenAIProvider {
    fn provider_type(&self) -> ModelProvider {
        ModelProvider::OpenAI
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
            .header("Authorization", format!("Bearer {}", api_key))
            .send()
            .await;

        Ok(response.is_ok())
    }
}

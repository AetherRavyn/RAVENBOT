//! Ollama model provider (local)

use async_trait::async_trait;
use ravenbot_core::ModelProvider;
use serde::{Deserialize, Serialize};

use super::{StreamChunk, ModelProviderTrait, ModelResponse, Message, ToolDefinition, ModelError, Usage, DeltaCallback, StreamAccumulator, streaming, ToolCall};

const BASE_URL: &str = "http://localhost:11434";

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
    /// Ollama expects tool definitions in its own flat format
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ToolParam>>,
    /// Ask a reasoning model to emit its thinking.
    ///
    /// Ollama gates this on an explicit `"think": true` rather than inferring it
    /// from the model name, so without the field a local thinking model answers
    /// silently. `enable_reasoning` used to be discarded at the top of `send_chat`
    /// and the response's `reasoning` hardcoded to `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    think: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
    /// Ollama native vision: base64 images ride directly on the message
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    images: Vec<String>,
    /// Assistant tool calls (Ollama native function calling)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    tool_calls: Option<Vec<OllamaToolCall>>,
    /// For role == "tool": which function produced the result
    #[serde(skip_serializing_if = "Option::is_none", default)]
    tool_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OllamaToolCall {
    #[serde(default)]
    id: Option<String>,
    function: OllamaFunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OllamaFunctionCall {
    name: String,
    /// Ollama accepts arguments as a JSON object (and historically a string)
    #[serde(default)]
    arguments: serde_json::Value,
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

/// Non-streaming response
#[derive(Debug, Deserialize)]
struct ChatResponse {
    message: Option<ResponseMessage>,
    #[serde(default)]
    prompt_eval_count: u64,
    #[serde(default)]
    eval_count: u64,
}

#[derive(Debug, Deserialize)]
struct ResponseMessage {
    #[allow(dead_code)]
    role: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    tool_calls: Vec<OllamaToolCall>,
}

pub struct OllamaProvider {
    base_url: Option<String>,
    model_override: Option<String>,
    client: reqwest::Client,
}

impl OllamaProvider {
    pub fn new(base_url: Option<String>) -> Self {
        Self {
            base_url,
            model_override: None,
            client: reqwest::Client::new(),
        }
    }

    fn base_url(&self) -> String {
        self.base_url.clone().unwrap_or_else(|| BASE_URL.to_string())
    }

    /// Shared send path for both non-streaming and streaming requests
    async fn send_chat(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        temperature: f32,
        _max_tokens: u32,
        stream: bool,
        on_delta: Option<&DeltaCallback>,
        enable_reasoning: bool,
    ) -> Result<ModelResponse, ModelError> {
        let _ = enable_reasoning;
        // Ollama resolves the model server-side via /api/tags; use its
        // preferred model when none was configured. max_tokens maps to
        // num_predict via options (kept simple here).
        let model = match &self.model_override {
            Some(m) => m.clone(),
            None => match std::env::var("OLLAMA_MODEL") {
                Ok(m) if !m.trim().is_empty() => m.trim().to_string(),
                _ => "llama3.1".to_string(),
            },
        };

        let system = messages
            .iter()
            .find(|m| m.role == "system")
            .map(|m| m.content.clone());

        let mut full_messages: Vec<ChatMessage> = Vec::new();
        if let Some(sys) = &system {
            full_messages.push(ChatMessage {
                role: "system".to_string(),
                content: sys.clone(),
                images: Vec::new(),
                tool_calls: None,
                tool_name: None,
            });
        }

        for m in messages.iter().filter(|m| m.role != "system") {
            // Tool result: Ollama keys it by tool_name on a "tool" role message.
            if m.role == "tool" {
                full_messages.push(ChatMessage {
                    role: "tool".to_string(),
                    content: m.content.clone(),
                    images: Vec::new(),
                    tool_calls: None,
                    tool_name: m.name.clone(),
                });
                continue;
            }

            // Assistant tool calls round-trip natively.
            let tool_calls = if m.has_tool_calls() {
                Some(
                    m.tool_calls
                        .iter()
                        .map(|tc| OllamaToolCall {
                            id: Some(tc.id.clone()),
                            function: OllamaFunctionCall {
                                name: tc.name.clone(),
                                arguments: tc.arguments.clone(),
                            },
                        })
                        .collect(),
                )
            } else {
                None
            };

            full_messages.push(ChatMessage {
                role: m.role.clone(),
                content: m.content.clone(),
                // Ollama native vision: base64 payload rides directly on the message
                images: m.images.iter().map(|img| img.data.clone()).collect(),
                tool_calls,
                tool_name: None,
            });
        }

        let tools_param = if tools.is_empty() {
            None
        } else {
            Some(
                tools
                    .iter()
                    .map(|t| ToolParam {
                        tool_type: "function".to_string(),
                        function: FunctionParam {
                            name: t.name.clone(),
                            description: t.description.clone(),
                            parameters: t.parameters.clone(),
                        },
                    })
                    .collect(),
            )
        };

        let request = ChatRequest {
            model,
            messages: full_messages,
            temperature,
            stream,
            tools: tools_param,
            think: if enable_reasoning { Some(true) } else { None },
        };

        let response = self
            .client
            .post(format!("{}/api/chat", self.base_url()))
            .json(&request)
            .send()
            .await
            .map_err(ModelError::Http)?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ModelError::Provider(format!(
                "Ollama error {}: {}",
                status, body
            )));
        }

        if let Some(on_delta) = on_delta {
            // Streaming path: Ollama streams NDJSON (one JSON object per line),
            // which consume_sse handles since bare lines are treated as payloads.
            let mut acc = StreamAccumulator::new();
            let mut tool_index = 0usize;
            streaming::consume_sse(response, |json| {
                if let Some(text) = json.pointer("/message/content").and_then(|v| v.as_str()) {
                    if !text.is_empty() {
                        acc.push_text(text);
                        on_delta(StreamChunk::Text(text));
                    }
                }
                // `thinking` is where Ollama puts a thinking model's reasoning.
                // It was never read, so a local model configured to reason gave a
                // silent answer with no trace of the thought behind it.
                if let Some(thinking) = json.pointer("/message/thinking").and_then(|v| v.as_str()) {
                    if !thinking.is_empty() {
                        acc.push_reasoning(thinking);
                        on_delta(StreamChunk::Reasoning(thinking));
                    }
                }
                // Ollama delivers complete tool calls (arguments already an
                // object) — previously these were dropped, so local bots
                // could never call tools.
                if let Some(calls) = json
                    .pointer("/message/tool_calls")
                    .and_then(|v| v.as_array())
                {
                    for call in calls {
                        let name = call
                            .pointer("/function/name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if name.is_empty() {
                            continue;
                        }
                        let args = normalize_ollama_arguments(
                            call.pointer("/function/arguments")
                                .cloned()
                                .unwrap_or_else(|| serde_json::json!({})),
                        );
                        let provided_id = call.get("id").and_then(|v| v.as_str()).unwrap_or("");
                        let id = if provided_id.is_empty() {
                            format!("call_{}", tool_index)
                        } else {
                            provided_id.to_string()
                        };
                        acc.push_tool_use_start(tool_index, &id, name);
                        acc.push_tool_json_delta(
                            tool_index,
                            &serde_json::to_string(&args).unwrap_or_else(|_| "{}".to_string()),
                        );
                        tool_index += 1;
                    }
                }
                let input = json.get("prompt_eval_count").and_then(|v| v.as_u64()).unwrap_or(0);
                let output = json.get("eval_count").and_then(|v| v.as_u64()).unwrap_or(0);
                if input > 0 || output > 0 {
                    acc.set_usage(input, output);
                }
                Ok(())
            })
            .await?;
            Ok(acc.finish())
        } else {
            // Non-streaming path
            let chat_response: ChatResponse = response.json().await.map_err(ModelError::Http)?;

            let content = chat_response
                .message
                .as_ref()
                .map(|m| m.content.clone())
                .unwrap_or_default();

            let tool_calls = chat_response
                .message
                .as_ref()
                .map(|m| {
                    m.tool_calls
                        .iter()
                        .enumerate()
                        .map(|(i, tc)| ToolCall {
                            name: tc.function.name.clone(),
                            arguments: normalize_ollama_arguments(tc.function.arguments.clone()),
                            id: tc
                                .id
                                .clone()
                                .filter(|s| !s.is_empty())
                                .unwrap_or_else(|| format!("call_{}", i)),
                        })
                        .collect()
                })
                .unwrap_or_default();

            Ok(ModelResponse {
                content: if content.is_empty() { None } else { Some(content) },
                tool_calls,
                usage: Usage {
                    input_tokens: chat_response.prompt_eval_count,
                    output_tokens: chat_response.eval_count,
                },
                reasoning: None,
            })
        }
    }
}

/// Ollama returns tool arguments as a JSON object, but some versions/models
/// emit them as a JSON string — normalize both to a `Value`.
fn normalize_ollama_arguments(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::String(s) => {
            serde_json::from_str(&s).unwrap_or(serde_json::Value::String(s))
        }
        other => other,
    }
}

#[async_trait]
impl ModelProviderTrait for OllamaProvider {
    fn provider_type(&self) -> ModelProvider {
        ModelProvider::Ollama
    }

    async fn complete(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        temperature: f32,
        max_tokens: u32,
    ) -> Result<ModelResponse, ModelError> {
        self.send_chat(messages, tools, temperature, max_tokens, false, None, false)
            .await
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
        let _ = enable_reasoning;
        self.send_chat(messages, tools, temperature, max_tokens, true, Some(&on_delta), enable_reasoning)
            .await
    }

    fn with_model(mut self: Box<Self>, model_id: String) -> Box<dyn ModelProviderTrait> {
        self.model_override = Some(model_id);
        Box::new(*self)
    }

    async fn health_check(&self) -> Result<bool, ModelError> {
        let resp = self
            .client
            .get(format!("{}/api/tags", self.base_url()))
            .send()
            .await;
        Ok(resp.is_ok())
    }
}

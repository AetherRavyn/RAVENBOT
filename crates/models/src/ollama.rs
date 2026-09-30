//! Ollama model provider (local)

use async_trait::async_trait;
use ravenbot_core::ModelProvider;
use serde::{Deserialize, Serialize};

use super::{ModelProviderTrait, ModelResponse, Message, ToolDefinition, ModelError, Usage, DeltaCallback, StreamAccumulator, streaming};

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
}

#[derive(Debug, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
    /// Ollama native vision: base64 images ride directly on the message
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    images: Vec<String>,
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
    content: String,
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

        let chat_messages: Vec<ChatMessage> = messages
            .iter()
            .filter(|m| m.role != "system")
            .map(|m| ChatMessage {
                role: m.role.clone(),
                content: m.content.clone(),
                images: Vec::new(),
            })
            .collect();

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
            });
        }
        // Ollama native vision: base64 payload rides directly on the message
        let chat_messages: Vec<ChatMessage> = chat_messages
            .into_iter()
            .zip(messages.iter().filter(|m| m.role != "system"))
            .map(|(mut msg, m)| {
                for img in &m.images {
                    msg.images.push(img.data.clone());
                }
                msg
            })
            .collect();
        full_messages.extend(chat_messages);

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
            streaming::consume_sse(response, |json| {
                if let Some(text) = json.pointer("/message/content").and_then(|v| v.as_str()) {
                    if !text.is_empty() {
                        acc.push_text(text);
                        on_delta(text);
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
                .map(|m| m.content)
                .unwrap_or_default();

            Ok(ModelResponse {
                content: if content.is_empty() { None } else { Some(content) },
                tool_calls: Vec::new(),
                usage: Usage {
                    input_tokens: chat_response.prompt_eval_count,
                    output_tokens: chat_response.eval_count,
                },
                reasoning: None,
            })
        }
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

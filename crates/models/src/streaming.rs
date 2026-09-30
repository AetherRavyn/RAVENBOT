//! Shared streaming infrastructure: SSE line consumption and
//! incremental response assembly for token-streaming providers.

use futures_util::StreamExt;
use reqwest::Response;
use serde_json::Value;

use super::{ModelError, ModelResponse, ToolCall, Usage};

/// Incrementally assembles a `ModelResponse` from streamed chunks.
pub struct StreamAccumulator {
    content: String,
    /// Chain-of-thought reasoning (extended thinking), kept separate from content
    reasoning: String,
    /// OpenAI/OpenRouter style: tool calls accumulated by index
    tools: std::collections::BTreeMap<usize, PartialTool>,
    usage: Option<Usage>,
}

#[derive(Default)]
struct PartialTool {
    id: String,
    name: String,
    arguments: String,
}

impl Default for StreamAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamAccumulator {
    pub fn new() -> Self {
        Self {
            content: String::new(),
            reasoning: String::new(),
            tools: std::collections::BTreeMap::new(),
            usage: None,
        }
    }

    /// Append streamed assistant text (forwarded to the UI as it arrives)
    pub fn push_text(&mut self, delta: &str) {
        self.content.push_str(delta);
    }

    /// Append streamed reasoning tokens (extended thinking), kept separate
    pub fn push_reasoning(&mut self, delta: &str) {
        self.reasoning.push_str(delta);
    }

    /// Merge an OpenAI/OpenRouter style tool-call delta chunk
    pub fn push_tool_call_delta(
        &mut self,
        index: Option<usize>,
        id: Option<&str>,
        name: Option<&str>,
        arguments_delta: Option<&str>,
    ) {
        let idx = index.unwrap_or(0);
        let tool = self.tools.entry(idx).or_default();
        if let Some(id) = id {
            if !id.is_empty() {
                tool.id = id.to_string();
            }
        }
        if let Some(name) = name {
            if !name.is_empty() {
                tool.name = name.to_string();
            }
        }
        if let Some(args) = arguments_delta {
            tool.arguments.push_str(args);
        }
    }

    /// Register an Anthropic-style `tool_use` block start
    pub fn push_tool_use_start(&mut self, index: usize, id: &str, name: &str) {
        self.tools.insert(
            index,
            PartialTool {
                id: id.to_string(),
                name: name.to_string(),
                arguments: String::new(),
            },
        );
    }

    /// Append an Anthropic-style `input_json_delta`
    pub fn push_tool_json_delta(&mut self, index: usize, partial_json: &str) {
        if let Some(tool) = self.tools.get_mut(&index) {
            tool.arguments.push_str(partial_json);
        }
    }

    pub fn set_usage(&mut self, input_tokens: u64, output_tokens: u64) {
        self.usage = Some(Usage {
            input_tokens,
            output_tokens,
        });
    }

    pub fn usage(&self) -> Option<&Usage> {
        self.usage.as_ref()
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    /// Assemble the final `ModelResponse`
    pub fn finish(self) -> ModelResponse {
        let tool_calls = self
            .tools
            .into_values()
            .filter_map(|tool| {
                let arguments = if tool.arguments.trim().is_empty() {
                    serde_json::Value::Object(serde_json::Map::new())
                } else {
                    serde_json::from_str(&tool.arguments).ok()?
                };
                Some(ToolCall {
                    name: tool.name,
                    arguments,
                    id: tool.id,
                })
            })
            .collect();

        let usage = self.usage.unwrap_or(Usage {
            input_tokens: 0,
            output_tokens: 0,
        });

        ModelResponse {
            content: if self.content.is_empty() {
                None
            } else {
                Some(self.content)
            },
            tool_calls,
            usage,
            reasoning: if self.reasoning.is_empty() { None } else { Some(self.reasoning) },
        }
    }
}

/// Consume an SSE response body, invoking `on_data` with each parsed
/// `data:` JSON payload. Handles multi-line buffering, `[DONE]` sentinels
/// and keep-alive comment lines (`: ...`).
pub async fn consume_sse<F>(response: Response, mut on_data: F) -> Result<(), ModelError>
where
    F: FnMut(&Value) -> Result<(), ModelError>,
{
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
        let bytes = chunk?;
        buffer.push_str(&String::from_utf8_lossy(&bytes));

        while let Some(pos) = buffer.find('\n') {
            let line: String = buffer.drain(..=pos).collect();
            let line = line.trim_end();

            // SSE comment / keep-alive lines
            if line.is_empty() || line.starts_with(':') {
                continue;
            }

            let payload = line.strip_prefix("data:").unwrap_or(line).trim();
            if payload.is_empty() {
                continue;
            }
            if payload == "[DONE]" {
                return Ok(());
            }
            let json: Value = serde_json::from_str(payload).map_err(ModelError::Serialization)?;
            on_data(&json)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accumulates_text_and_tool_call_deltas() {
        let mut acc = StreamAccumulator::new();
        acc.push_text("Hello");
        acc.push_text(" world");
        acc.push_tool_call_delta(
            Some(0),
            Some("call-1"),
            Some("web_search"),
            Some("{\"qu"),
        );
        acc.push_tool_call_delta(
            Some(0),
            None,
            None,
            Some("ery\": \"rust streaming\"}"),
        );

        let resp = acc.finish();
        assert_eq!(resp.content.as_deref(), Some("Hello world"));
        assert_eq!(resp.tool_calls.len(), 1);
        assert_eq!(resp.tool_calls[0].name, "web_search");
        assert_eq!(resp.tool_calls[0].id, "call-1");
        assert_eq!(
            resp.tool_calls[0].arguments.get("query").and_then(|v| v.as_str()),
            Some("rust streaming")
        );
        assert_eq!(resp.usage.output_tokens, 0);
    }

    #[test]
    fn anthropic_style_tool_use_blocks() {
        let mut acc = StreamAccumulator::new();
        acc.push_tool_use_start(2, "toolu-01", "git_status");
        acc.push_tool_json_delta(2, "{}");
        acc.set_usage(120, 45);

        let resp = acc.finish();
        assert_eq!(resp.tool_calls.len(), 1);
        assert_eq!(resp.tool_calls[0].name, "git_status");
        assert_eq!(resp.tool_calls[0].arguments, serde_json::json!({}));
        assert_eq!(resp.usage.input_tokens, 120);
        assert_eq!(resp.usage.output_tokens, 45);
    }

    #[test]
    fn empty_accumulator_yields_no_content() {
        let resp = StreamAccumulator::new().finish();
        assert!(resp.content.is_none());
        assert!(resp.tool_calls.is_empty());
    }

    #[test]
    fn reasoning_is_kept_separate_from_content() {
        let mut acc = StreamAccumulator::new();
        acc.push_reasoning("Let me trace the edge case: ");
        acc.push_reasoning("dividing by zero.");
        acc.push_text("The answer is 42.");

        let resp = acc.finish();
        assert_eq!(resp.content.as_deref(), Some("The answer is 42."));
        assert_eq!(
            resp.reasoning.as_deref(),
            Some("Let me trace the edge case: dividing by zero.")
        );
    }

    #[test]
    fn reasoning_only_response_has_no_content() {
        let mut acc = StreamAccumulator::new();
        acc.push_reasoning("pure deliberation");
        let resp = acc.finish();
        assert!(resp.content.is_none());
        assert_eq!(resp.reasoning.as_deref(), Some("pure deliberation"));
    }
}


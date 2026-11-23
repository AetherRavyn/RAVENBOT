//! Shared streaming infrastructure: SSE line consumption and
//! incremental response assembly for token-streaming providers.

use futures_util::StreamExt;
use reqwest::Response;
use serde_json::Value;

use super::{ModelError, ModelResponse, ToolCall, Usage};

/// Pull reasoning tokens out of an OpenAI-shaped streaming delta.
///
/// Every provider that speaks the OpenAI wire format ships its chain of thought
/// under a slightly different key, and no two agree:
///
///  - `reasoning_content` — DeepSeek, Moonshot/Kimi, Qwen, most gateways
///  - `reasoning` — OpenRouter, some proxies
///  - `thinking` — a handful of Anthropic-compatible bridges
///
/// All three are checked in that order. This is the reason reasoning was
/// Anthropic-only in practice: the OpenAI-shaped providers took
/// `_enable_reasoning` — ignored — and returned a hardcoded `reasoning: None`,
/// so a model doing exactly what it was asked and emitting its reasoning got it
/// thrown away. One helper, used by all of them, is what stops that being fixed
/// three times and forgotten once.
///
/// Returns an empty string rather than `None` when the key is absent, so callers
/// can forward it unconditionally without a branch on every chunk.
pub fn openai_reasoning_delta(delta: &Value) -> &str {
    ["reasoning_content", "reasoning", "thinking"]
        .iter()
        .find_map(|k| delta.get(*k).and_then(|v| v.as_str()))
        .unwrap_or("")
}

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
            reasoning: if self.reasoning.is_empty() {
                None
            } else {
                Some(self.reasoning)
            },
        }
    }
}

/// Consume an SSE response body, invoking `on_data` with each parsed
/// `data:` JSON payload.
///
/// SSE has named fields (`event:`, `id:`, `retry:`) as well as `data:`.
/// Anthropic sends an `event:` line before every payload, so treating every
/// non-empty line as JSON made every Anthropic stream fail on `message_start`.
/// Field lines are now ignored unless they are `data:`. Bare JSON lines are
/// still accepted because Ollama's stream is newline-delimited JSON rather than
/// SSE.
///
/// Chunks are buffered as bytes and decoded only after a complete line is
/// assembled. Decoding each network chunk separately replaced a multi-byte
/// UTF-8 character split across two chunks with `�`.
/// Returns whether the explicit `[DONE]` sentinel was seen. Providers that use
/// another terminal signal (Anthropic's `message_stop`, Ollama's `done`) track
/// it in their own callback and combine it with this result.
pub async fn consume_sse<F>(response: Response, mut on_data: F) -> Result<bool, ModelError>
where
    F: FnMut(&Value) -> Result<(), ModelError>,
{
    let mut stream = response.bytes_stream();
    let mut buffer: Vec<u8> = Vec::new();

    while let Some(chunk) = stream.next().await {
        buffer.extend_from_slice(&chunk?);

        while let Some(pos) = buffer.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = buffer.drain(..=pos).collect();
            if consume_sse_line(&line, &mut on_data)? {
                return Ok(true);
            }
        }
    }

    // A final line without a trailing newline is still a line.
    if !buffer.is_empty() && consume_sse_line(&buffer, &mut on_data)? {
        return Ok(true);
    }
    Ok(false)
}

/// Handle one line. Returns `true` when the `[DONE]` sentinel was seen.
fn consume_sse_line<F>(line: &[u8], on_data: &mut F) -> Result<bool, ModelError>
where
    F: FnMut(&Value) -> Result<(), ModelError>,
{
    let line = std::str::from_utf8(line)
        .map_err(|error| ModelError::Provider(format!("invalid UTF-8 in stream: {error}")))?;
    let line = line.trim_end_matches(['\r', '\n']).trim();

    // Comments, keep-alives, and named SSE fields that are not payloads.
    if line.is_empty() || line.starts_with(':') {
        return Ok(false);
    }
    let Some(payload) = line.strip_prefix("data:") else {
        // Ollama's newline-delimited JSON has no `data:` prefix. A non-JSON
        // field line (`event:`, `id:`, `retry:`) is metadata, not an error.
        if !line.starts_with('{') && !line.starts_with('[') {
            return Ok(false);
        }
        let json: Value = serde_json::from_str(line).map_err(ModelError::Serialization)?;
        on_data(&json)?;
        return Ok(false);
    };
    let payload = payload.trim();
    if payload.is_empty() {
        return Ok(false);
    }
    if payload == "[DONE]" {
        return Ok(true);
    }
    let json: Value = serde_json::from_str(payload).map_err(ModelError::Serialization)?;
    on_data(&json)?;
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accumulates_text_and_tool_call_deltas() {
        let mut acc = StreamAccumulator::new();
        acc.push_text("Hello");
        acc.push_text(" world");
        acc.push_tool_call_delta(Some(0), Some("call-1"), Some("web_search"), Some("{\"qu"));
        acc.push_tool_call_delta(Some(0), None, None, Some("ery\": \"rust streaming\"}"));

        let resp = acc.finish();
        assert_eq!(resp.content.as_deref(), Some("Hello world"));
        assert_eq!(resp.tool_calls.len(), 1);
        assert_eq!(resp.tool_calls[0].name, "web_search");
        assert_eq!(resp.tool_calls[0].id, "call-1");
        assert_eq!(
            resp.tool_calls[0]
                .arguments
                .get("query")
                .and_then(|v| v.as_str()),
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
    fn sse_field_lines_are_not_parsed_as_json() {
        let mut payloads = Vec::new();
        let mut on_data = |value: &Value| {
            payloads.push(value.clone());
            Ok(())
        };
        // Anthropic-shaped stream: event lines bracket each data payload.
        assert!(!consume_sse_line(b"event: message_start\n", &mut on_data).unwrap());
        assert!(!consume_sse_line(br#"data: {"type":"message_start"}"#, &mut on_data).unwrap());
        assert!(!consume_sse_line(b"event: content_block_delta\n", &mut on_data).unwrap());
        assert!(!consume_sse_line(
            br#"data: {"type":"content_block_delta","delta":{"text":"hi"}}"#,
            &mut on_data
        )
        .unwrap());
        assert!(consume_sse_line(b"data: [DONE]", &mut on_data).unwrap());
        assert_eq!(payloads.len(), 2);
        assert_eq!(payloads[1]["delta"]["text"], "hi");
    }

    #[test]
    fn ollama_bare_json_lines_still_parse() {
        let mut payloads = Vec::new();
        let mut on_data = |value: &Value| {
            payloads.push(value.clone());
            Ok(())
        };
        assert!(!consume_sse_line(br#"{"message":{"content":"hi"}}"#, &mut on_data).unwrap());
        assert_eq!(payloads.len(), 1);
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

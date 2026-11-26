//! Turn a completed run's tool trace into a reviewed, versioned skill draft.
//!
//! This is the first learning loop in RAVENBOT, and it is deliberately narrow:
//!
//! - It learns from **tool calls**, not from prose, reasoning, or tool output.
//! - It runs deterministically. There is no model call, so the same trace
//!   produces the same draft and a failed provider cannot invent a step.
//! - It proposes; it never saves. The user edits the draft and explicitly
//!   saves it through [`crate::local::LocalSkillLibrary::create_with_source`].
//! - It redacts credentials before a value can enter the draft, and it drops
//!   payload fields (typed text, file contents, request bodies) entirely.
//!
//! What a run is allowed to teach is intentionally modest: "these tools were
//! called, in this order, to make progress on this goal". It does not claim the
//! steps were verified, minimal, or sufficient. Human demonstration capture and
//! replay are separate, later increments.

use ravenbot_core::ToolTrace;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::sync::OnceLock;

/// Most steps one draft carries. A longer trace is truncated, with a warning,
/// because a 200-step "skill" is not reviewable and not a skill.
pub const MAX_LEARNED_STEPS: usize = 50;

/// Placeholder written into any value that looks like a credential.
pub const REDACTED: &str = "[redacted]";

/// A step the run actually performed, with its arguments sanitized.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LearnedStep {
    /// The skill/tool name as the model called it.
    pub tool: String,
    /// One browser/desktop action, or a `→`-joined sequence for a grouped step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// A short human-readable description of the step.
    pub summary: String,
    /// Sanitized arguments. Never a raw tool result.
    pub arguments: Value,
}

/// A proposed skill. The content is editable; the other fields describe the
/// synthesis so the review screen can explain itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillDraft {
    /// Filesystem-safe id the library would use. The user may edit it.
    pub id: String,
    /// Human-readable title shown in the library.
    pub name: String,
    /// One-line description used by the frontmatter and skill list.
    pub description: String,
    /// Prompt a user could use to run the skill again.
    pub example_prompt: String,
    /// A complete `SKILL.md`, including YAML frontmatter.
    pub content: String,
    /// The happy-path steps, in order, capped at [`MAX_LEARNED_STEPS`].
    pub steps: Vec<LearnedStep>,
    /// Distinct tools used by the included steps, in first-use order.
    pub tools: Vec<String>,
    /// Things the reviewer must know: failed calls, truncation, redaction,
    /// omitted payloads, and high-risk tools.
    pub warnings: Vec<String>,
    /// The run this draft came from. Used to record provenance.
    pub source_run_id: String,
    /// The agent that produced the run, for the evidence section.
    pub source_bot_name: String,
}

/// Synthesize a skill draft from the tool traces of one successful run.
///
/// `goal` is normally the user message that started the run. Every string that
/// enters the draft passes through [`redact_text`]; every argument object passes
/// through [`redact_json`]. Failed calls are excluded from the steps and named
/// in warnings. Raw tool results are never read.
pub fn propose_skill_from_traces(
    goal: &str,
    bot_name: &str,
    run_id: &str,
    traces: &[ToolTrace],
) -> Result<SkillDraft, String> {
    if traces.is_empty() {
        return Err("This run has no tool trace to learn from.".to_string());
    }

    let bot_name = redact_text(&clean_inline(bot_name, 80));
    let run_id = redact_text(&clean_inline(run_id, 64));
    // Redact before any branch can copy the goal into a name, description or
    // frontmatter field. The name builder redacts again; idempotence is cheap
    // and the earlier pass is what keeps every use of `goal` safe.
    let goal = redact_text(&clean_inline(goal, 300));

    let mut failed: Vec<String> = Vec::new();
    let mut successful: Vec<&ToolTrace> = Vec::new();
    for trace in traces {
        if trace.is_error {
            failed.push(trace.name.clone());
        } else {
            successful.push(trace);
        }
    }
    if successful.is_empty() {
        return Err(format!(
            "All {} tool call(s) in this run failed, so there is no happy path to learn.",
            traces.len()
        ));
    }

    let (successful, collapsed) = dedupe_consecutive(successful);
    let mut warnings = Vec::new();
    let mut redacted_any = false;
    let mut omitted_any = false;

    if collapsed > 0 {
        warnings.push(format!(
            "{collapsed} repeated identical tool call(s) were collapsed into one step."
        ));
    }
    if !failed.is_empty() {
        let names = failed.into_iter().collect::<BTreeSet<_>>();
        warnings.push(format!(
            "{} failed tool call(s) were excluded from the happy path ({}).",
            names.len(),
            names.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }

    let mut steps = Vec::new();
    let mut index = 0;
    while index < successful.len() {
        let trace = successful[index];

        if trace.name == "browser_dom" {
            if let Some(action) = action_of(&trace.arguments) {
                if BROWSER_SEQUENCE.contains(&action.as_str()) {
                    let start = index;
                    while index < successful.len() && successful[index].name == "browser_dom" {
                        match action_of(&successful[index].arguments) {
                            Some(next) if BROWSER_SEQUENCE.contains(&next.as_str()) => {
                                index += 1;
                            }
                            _ => break,
                        }
                    }
                    let (step, redacted, omitted) = browser_step(&successful[start..index]);
                    redacted_any |= redacted;
                    omitted_any |= omitted;
                    steps.push(step);
                    continue;
                }
            }
        }

        if trace.name == "computer_control" {
            let start = index;
            while index < successful.len() && successful[index].name == "computer_control" {
                index += 1;
            }
            let (step, redacted, omitted) = desktop_step(&successful[start..index]);
            redacted_any |= redacted;
            omitted_any |= omitted;
            steps.push(step);
            continue;
        }

        let (summary, redacted) = summarize_tool(&trace.name, &trace.arguments);
        redacted_any |= redacted;
        let (arguments, redacted_args, omitted) = sanitize_arguments(&trace.name, &trace.arguments);
        redacted_any |= redacted_args;
        omitted_any |= omitted;
        steps.push(LearnedStep {
            tool: trace.name.clone(),
            action: action_of(&trace.arguments),
            summary: redact_text(&summary),
            arguments,
        });
        index += 1;
    }

    if steps.is_empty() {
        return Err("This run has no successful tool steps to learn from.".to_string());
    }

    if steps.len() > MAX_LEARNED_STEPS {
        let omitted = steps.len() - MAX_LEARNED_STEPS;
        warnings.push(format!(
            "Only the first {MAX_LEARNED_STEPS} steps are included; {omitted} later step(s) were omitted."
        ));
        steps.truncate(MAX_LEARNED_STEPS);
    }

    if redacted_any {
        warnings.push(
            "Some argument values looked like credentials and were replaced with [redacted]."
                .to_string(),
        );
    }
    if omitted_any {
        warnings.push(
            "Typed text, file contents, request bodies and scripts were omitted from the draft. Re-enter any values the task needs."
                .to_string(),
        );
    }

    let risky = high_risk_labels(&steps);
    if !risky.is_empty() {
        warnings.push(format!(
            "This skill uses high-risk tools: {}. Review every step; each run still follows the approval policy.",
            risky.join(", ")
        ));
    }

    let tools = tools_in_order(&steps);
    let id = draft_id(&goal, &run_id);
    let name = draft_name(&goal, &bot_name);
    let description = truncate_chars(
        &format!("A reviewed workflow learned from a completed {bot_name} run: {goal}"),
        240,
    );
    let example_prompt = if goal.trim().is_empty() {
        format!("Repeat the workflow from run {run_id}.")
    } else {
        goal.clone()
    };

    let content = build_content(
        &name,
        &description,
        &example_prompt,
        &run_id,
        &bot_name,
        &steps,
        &tools,
        &warnings,
        &risky,
        omitted_any,
    );

    Ok(SkillDraft {
        id,
        name,
        description,
        example_prompt,
        content,
        steps,
        tools,
        warnings,
        source_run_id: run_id,
        source_bot_name: bot_name,
    })
}

// ── trace shaping ─────────────────────────────────────────────────────────

/// Browser actions grouped into one readable interaction step. A snapshot is
/// usually the prerequisite for the next action, not a step a user repeats.
const BROWSER_SEQUENCE: &[&str] = &["snapshot", "click", "fill", "press"];

fn action_of(arguments: &Value) -> Option<String> {
    arguments
        .get("action")
        .and_then(Value::as_str)
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
}

/// Drop a call when it is byte-for-byte the same call as the one before it.
/// A model retry loop is noise in a skill, not a second step.
fn dedupe_consecutive(traces: Vec<&ToolTrace>) -> (Vec<&ToolTrace>, usize) {
    let mut out: Vec<&ToolTrace> = Vec::with_capacity(traces.len());
    let mut collapsed = 0;
    for trace in traces {
        match out.last() {
            Some(previous)
                if previous.name == trace.name && previous.arguments == trace.arguments =>
            {
                collapsed += 1;
            }
            _ => out.push(trace),
        }
    }
    (out, collapsed)
}

fn browser_step(sequence: &[&ToolTrace]) -> (LearnedStep, bool, bool) {
    let mut actions = Vec::with_capacity(sequence.len());
    let mut phrases = Vec::with_capacity(sequence.len());
    let mut action_names = Vec::with_capacity(sequence.len());
    let mut redacted_any = false;
    let mut omitted_any = false;

    for trace in sequence {
        let action = action_of(&trace.arguments).unwrap_or_default();
        let (phrase, redacted) = browser_phrase(&action, &trace.arguments);
        redacted_any |= redacted;
        let (arguments, redacted_args, omitted) =
            sanitize_arguments("browser_dom", &trace.arguments);
        redacted_any |= redacted_args;
        omitted_any |= omitted;
        action_names.push(if action.is_empty() {
            "action".to_string()
        } else {
            action
        });
        phrases.push(phrase);
        actions.push(arguments);
    }

    let summary = phrases
        .into_iter()
        .map(|phrase| redact_text(&phrase))
        .collect::<Vec<_>>()
        .join(", then ");
    (
        LearnedStep {
            tool: "browser_dom".to_string(),
            action: Some(action_names.join(" → ")),
            summary: truncate_chars(&summary, 400),
            arguments: json!({ "actions": actions }),
        },
        redacted_any,
        omitted_any,
    )
}

fn desktop_step(sequence: &[&ToolTrace]) -> (LearnedStep, bool, bool) {
    let mut actions = Vec::with_capacity(sequence.len());
    let mut phrases = Vec::with_capacity(sequence.len());
    let mut action_names = Vec::with_capacity(sequence.len());
    let mut redacted_any = false;
    let mut omitted_any = false;

    for trace in sequence {
        let action = action_of(&trace.arguments).unwrap_or_default();
        let (phrase, redacted) = desktop_phrase(&action, &trace.arguments);
        redacted_any |= redacted;
        let (arguments, redacted_args, omitted) =
            sanitize_arguments("computer_control", &trace.arguments);
        redacted_any |= redacted_args;
        omitted_any |= omitted;
        action_names.push(if action.is_empty() {
            "action".to_string()
        } else {
            action
        });
        phrases.push(phrase);
        actions.push(arguments);
    }

    let summary = phrases
        .into_iter()
        .map(|phrase| redact_text(&phrase))
        .collect::<Vec<_>>()
        .join(", then ");
    (
        LearnedStep {
            tool: "computer_control".to_string(),
            action: Some(action_names.join(" → ")),
            summary: truncate_chars(&summary, 400),
            arguments: json!({ "actions": actions }),
        },
        redacted_any,
        omitted_any,
    )
}

fn browser_phrase(action: &str, arguments: &Value) -> (String, bool) {
    let selector = arguments
        .get("selector")
        .and_then(Value::as_str)
        .map(clean_selector)
        .filter(|value| !value.is_empty());
    let (phrase, redacted) = match action {
        "snapshot" => ("Inspect the page snapshot".to_string(), false),
        "click" => match &selector {
            Some(selector) => (format!("Click {selector}"), false),
            None => ("Click on the page".to_string(), false),
        },
        "fill" => match &selector {
            Some(selector) => (format!("Fill {selector} (typed value omitted)"), false),
            None => (
                "Fill the focused field (typed value omitted)".to_string(),
                false,
            ),
        },
        "press" => {
            let key = arguments
                .get("key")
                .and_then(Value::as_str)
                .map(|value| clean_inline(value, 40))
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "a key".to_string());
            let modifiers = arguments
                .get("modifiers")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(|value| clean_inline(value, 20))
                        .filter(|value| !value.is_empty())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if modifiers.is_empty() {
                (format!("Press {key}"), false)
            } else {
                (format!("Press {}+{key}", modifiers.join("+")), false)
            }
        }
        "scroll" => ("Scroll the page".to_string(), false),
        "highlight" => match &selector {
            Some(selector) => (format!("Highlight {selector}"), false),
            None => ("Highlight page content".to_string(), false),
        },
        "text" => ("Read the page text".to_string(), false),
        "html" => ("Read the page HTML".to_string(), false),
        "back" => ("Go back".to_string(), false),
        "forward" => ("Go forward".to_string(), false),
        "reload" => ("Reload the page".to_string(), false),
        "wait" => ("Wait for the page".to_string(), false),
        "open" => match arguments.get("url").and_then(Value::as_str) {
            Some(url) => {
                let cleaned = redact_text(&clean_inline(url, 200));
                (format!("Open {cleaned}"), cleaned != url)
            }
            None => ("Open a page".to_string(), false),
        },
        "eval" => ("Evaluate a page script (script omitted)".to_string(), false),
        other => {
            let other = clean_inline(other, 40);
            (format!("Run browser action {other}"), false)
        }
    };
    (phrase, redacted)
}

fn desktop_phrase(action: &str, arguments: &Value) -> (String, bool) {
    let point = || {
        let x = arguments.get("x").and_then(Value::as_f64);
        let y = arguments.get("y").and_then(Value::as_f64);
        match (x, y) {
            (Some(x), Some(y)) => format!(" at ({x:.0}, {y:.0})"),
            _ => String::new(),
        }
    };
    match action {
        "list_windows" => ("List the open windows".to_string(), false),
        "focus" => {
            let window = arguments
                .get("window_id")
                .and_then(Value::as_u64)
                .map(|id| format!(" window {id}"))
                .or_else(|| {
                    arguments
                        .get("title")
                        .and_then(Value::as_str)
                        .map(|title| format!(" {}", clean_inline(title, 80)))
                })
                .unwrap_or_default();
            (format!("Focus{window}"), false)
        }
        "screenshot" => ("Take a screenshot".to_string(), false),
        "click" => (format!("Click{}", point()), false),
        "double_click" => (format!("Double-click{}", point()), false),
        "right_click" => (format!("Right-click{}", point()), false),
        "move_mouse" => (format!("Move the pointer{}", point()), false),
        "scroll" => (format!("Scroll{}", point()), false),
        "type" => (
            "Type into the active window (text omitted)".to_string(),
            false,
        ),
        "key" => {
            let key = arguments
                .get("key")
                .and_then(Value::as_str)
                .map(|value| clean_inline(value, 40))
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "a key".to_string());
            (format!("Press {key}"), false)
        }
        "wait" => {
            let ms = arguments.get("milliseconds").and_then(Value::as_u64);
            match ms {
                Some(ms) => (format!("Wait {ms} ms"), false),
                None => ("Wait".to_string(), false),
            }
        }
        other => {
            let other = clean_inline(other, 40);
            (format!("Run desktop action {other}"), false)
        }
    }
}

/// Summaries for tools that are not grouped. A summary names the workflow
/// object (command, path, query, URL), never the tool's result.
fn summarize_tool(name: &str, arguments: &Value) -> (String, bool) {
    let text = |key: &str, max: usize| {
        arguments
            .get(key)
            .and_then(Value::as_str)
            .map(|value| clean_inline(value, max))
            .filter(|value| !value.is_empty())
    };
    let redacted = |value: String| {
        let cleaned = redact_text(&value);
        let changed = cleaned != value;
        (cleaned, changed)
    };

    match name {
        "shell_exec" | "shell" | "bash" => match text("command", 180) {
            Some(command) => redacted(format!("Run {command}")),
            None => ("Run a shell command".to_string(), false),
        },
        "file_read" => match text("path", 160) {
            Some(path) => redacted(format!("Read {path}")),
            None => ("Read a file".to_string(), false),
        },
        "file_write" => match text("path", 160) {
            Some(path) => redacted(format!("Write {path}")),
            None => ("Write a file".to_string(), false),
        },
        "file_edit" | "code_edit" => match text("path", 160) {
            Some(path) => redacted(format!("Edit {path}")),
            None => ("Edit a file".to_string(), false),
        },
        "file_tree" => match text("path", 160) {
            Some(path) => redacted(format!("List files in {path}")),
            None => ("List files".to_string(), false),
        },
        "code_search" => match text("query", 120).or_else(|| text("pattern", 120)) {
            Some(query) => redacted(format!("Search the code for {query}")),
            None => ("Search the code".to_string(), false),
        },
        "web_search" | "tavily_search" => match text("query", 160) {
            Some(query) => redacted(format!("Search the web for {query}")),
            None => ("Search the web".to_string(), false),
        },
        "http_request" => {
            let method = text("method", 12)
                .map(|value| value.to_ascii_uppercase())
                .unwrap_or_else(|| "GET".to_string());
            match text("url", 200) {
                Some(url) => redacted(format!("Send a {method} request to {url}")),
                None => (format!("Send a {method} request"), false),
            }
        }
        "browser_open" | "browser_navigate" => match text("url", 200) {
            Some(url) => redacted(format!("Open {url}")),
            None => ("Open the browser".to_string(), false),
        },
        "ask_user" => ("Ask the user a question".to_string(), false),
        "delegate" => ("Delegate work to another agent".to_string(), false),
        "todo" => ("Update the todo list".to_string(), false),
        "memory_tool" => ("Update memory".to_string(), false),
        other if is_mcp_tool(other) => {
            (format!("Call the {} tool", clean_inline(other, 80)), false)
        }
        other => (format!("Use the {} tool", clean_inline(other, 80)), false),
    }
}

fn is_mcp_tool(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("mcp") || lower.contains("__")
}

// ── sanitizing arguments ──────────────────────────────────────────────────

/// Fields that carry user input or file/request payloads rather than the shape
/// of the workflow. They are never copied into a draft.
const PAYLOAD_KEYS: &[&str] = &[
    "text",
    "content",
    "script",
    "patch",
    "body",
    "new_content",
    "old_content",
    "file_content",
    "html",
    "document",
    "code",
    "data",
];

/// Cap on a step's serialized arguments. A step is for review, not a file dump.
const MAX_STEP_ARGS_BYTES: usize = 2_000;

fn sanitize_arguments(tool: &str, arguments: &Value) -> (Value, bool, bool) {
    let mut omitted = false;
    let mut base = arguments.clone();
    // Strip payload fields at every depth. A nested `text` is still typed text,
    // and an MCP tool can nest its arguments inside an envelope.
    strip_payload_keys(&mut base, &mut omitted);
    let (redacted, changed) = redact_json_tracked(&base);
    let rendered_len = serde_json::to_string(&redacted)
        .map(|value| value.len())
        .unwrap_or(0);
    let final_value = if rendered_len > MAX_STEP_ARGS_BYTES {
        omitted = true;
        json!({
            "tool": clean_inline(tool, 80),
            "note": "Arguments omitted from the draft because they were too large."
        })
    } else {
        redacted
    };
    (final_value, changed, omitted)
}

fn strip_payload_keys(value: &mut Value, omitted: &mut bool) {
    match value {
        Value::Object(map) => {
            for key in PAYLOAD_KEYS {
                if map.remove(*key).is_some() {
                    *omitted = true;
                }
            }
            for item in map.values_mut() {
                strip_payload_keys(item, omitted);
            }
        }
        Value::Array(items) => {
            for item in items {
                strip_payload_keys(item, omitted);
            }
        }
        _ => {}
    }
}

// ── redaction ─────────────────────────────────────────────────────────────

/// Does this JSON key name something that must never be copied verbatim?
///
/// Matching is deliberately broad: `session_id`, `token_count` and
/// `x-api-key` are all redacted. Over-redacting a harmless field costs one
/// reviewable `[redacted]`; under-redacting a credential ends the trust story.
pub fn is_sensitive_key(key: &str) -> bool {
    let normalized = key
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect::<String>();
    const MARKERS: &[&str] = &[
        "password",
        "passwd",
        "secret",
        "token",
        "apikey",
        "authorization",
        "cookie",
        "privatekey",
        "session",
        "credential",
    ];
    MARKERS.iter().any(|marker| normalized.contains(marker))
}

/// Recursively redact sensitive keys and credential-shaped strings in a JSON
/// value.
pub fn redact_json(value: &Value) -> Value {
    redact_json_tracked(value).0
}

fn redact_json_tracked(value: &Value) -> (Value, bool) {
    match value {
        Value::Object(map) => {
            let mut changed = false;
            let mut out = serde_json::Map::with_capacity(map.len());
            for (key, item) in map {
                if is_sensitive_key(key) {
                    out.insert(key.clone(), Value::String(REDACTED.to_string()));
                    changed = true;
                } else {
                    let (redacted, item_changed) = redact_json_tracked(item);
                    changed |= item_changed;
                    out.insert(key.clone(), redacted);
                }
            }
            (Value::Object(out), changed)
        }
        Value::Array(items) => {
            let mut changed = false;
            let values = items
                .iter()
                .map(|item| {
                    let (redacted, item_changed) = redact_json_tracked(item);
                    changed |= item_changed;
                    redacted
                })
                .collect();
            (Value::Array(values), changed)
        }
        Value::String(text) => {
            let redacted = redact_text(text);
            let changed = redacted != *text;
            (Value::String(redacted), changed)
        }
        other => (other.clone(), false),
    }
}

/// Redact common credential shapes inside free text, plus `KEY=value`
/// assignments whose key is sensitive.
pub fn redact_text(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let patterns = redaction_patterns();
    let mut out = text.to_string();
    out = patterns
        .bearer
        .replace_all(&out, "Bearer [redacted]")
        .into_owned();
    for pattern in &patterns.tokens {
        out = pattern.replace_all(&out, REDACTED).into_owned();
    }
    out = patterns
        .assignment
        .replace_all(&out, "${1}=[redacted]")
        .into_owned();
    out
}

struct RedactionPatterns {
    bearer: Regex,
    tokens: Vec<Regex>,
    assignment: Regex,
}

fn redaction_patterns() -> &'static RedactionPatterns {
    static PATTERNS: OnceLock<RedactionPatterns> = OnceLock::new();
    PATTERNS.get_or_init(|| RedactionPatterns {
        // `Bearer eyJ…` must lose the token but keep the scheme readable.
        bearer: Regex::new(r"(?i)\bBearer\s+[A-Za-z0-9\-._~+/]{8,}={0,2}")
            .expect("valid bearer pattern"),
        tokens: [
            // OpenAI / Anthropic style keys, including sk-proj-… variants.
            r"\bsk-[A-Za-z0-9_-]{8,}",
            // GitHub personal, OAuth, app and fine-grained tokens.
            r"\b(?:gh[pousr]|github_pat)_[A-Za-z0-9_]{8,}",
            // Slack bot/user/app/refresh tokens.
            r"\bxox[baprs]-[A-Za-z0-9-]{8,}",
            // AWS access key ids (the secret is a separate value).
            r"\b(?:AKIA|ASIA|AGPA|AIDA|AROA|AIPA|ANPA|ANVA)[A-Z0-9]{16}\b",
            // Stripe-style live/test keys.
            r"\b(?:sk|pk|rk)_(?:live|test)_[A-Za-z0-9]{8,}",
        ]
        .iter()
        .map(|pattern| Regex::new(pattern).expect("valid token pattern"))
        .collect(),
        // FOO_TOKEN=…, api_key=…, "PASSWORD":"…" etc. The value is replaced,
        // the key stays so a reviewer can see which field was removed.
        assignment: Regex::new(
            r#"(?i)([A-Za-z0-9_]*(?:PASSWORD|PASSWD|SECRET|TOKEN|API_KEY|APIKEY|PRIVATE_KEY|CREDENTIAL|SESSION)[A-Za-z0-9_]*)\s*=\s*(?:"[^"\n]*"|'[^'\n]*'|[^\s&;,]{4,})"#,
        )
        .expect("valid assignment pattern"),
    })
}

// ── high-risk classification ──────────────────────────────────────────────

fn high_risk_labels(steps: &[LearnedStep]) -> Vec<String> {
    let mut labels: Vec<String> = Vec::new();
    for step in steps {
        let label = match step.tool.as_str() {
            "shell_exec" | "computer_control" | "file_write" | "file_edit" | "code_edit" => {
                Some(step.tool.clone())
            }
            "browser_dom" => {
                let actions = step
                    .action
                    .as_deref()
                    .unwrap_or_default()
                    .split('→')
                    .map(|action| action.trim())
                    .filter(|action| {
                        matches!(*action, "click" | "fill" | "press" | "reload" | "eval")
                    })
                    .collect::<BTreeSet<_>>();
                if actions.is_empty() {
                    None
                } else {
                    Some(format!(
                        "browser_dom ({})",
                        actions.into_iter().collect::<Vec<_>>().join(", ")
                    ))
                }
            }
            other if is_mcp_tool(other) && is_write_like(other) => Some(other.to_string()),
            "http_request" => {
                let method = step
                    .arguments
                    .get("method")
                    .and_then(Value::as_str)
                    .unwrap_or("GET")
                    .to_ascii_uppercase();
                if matches!(method.as_str(), "GET" | "HEAD" | "OPTIONS") {
                    None
                } else {
                    Some(format!("http_request ({method})"))
                }
            }
            _ => None,
        };
        if let Some(label) = label {
            if !labels.contains(&label) {
                labels.push(label);
            }
        }
    }
    labels
}

fn is_write_like(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [
        "write", "create", "update", "delete", "remove", "send", "post", "put", "patch", "insert",
        "upload", "publish", "archive", "merge", "execute",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

fn tools_in_order(steps: &[LearnedStep]) -> Vec<String> {
    let mut tools = Vec::new();
    for step in steps {
        if !tools.contains(&step.tool) {
            tools.push(step.tool.clone());
        }
    }
    tools
}

// ── document synthesis ────────────────────────────────────────────────────

fn draft_name(goal: &str, bot_name: &str) -> String {
    let cleaned = redact_text(goal);
    let cleaned = cleaned
        .trim()
        .trim_matches(['.', '!', '?', '。', '！', '？']);
    if cleaned.is_empty() {
        return truncate_chars(&format!("Workflow from {bot_name}"), 80);
    }
    let mut chars = cleaned.chars();
    let first = chars
        .next()
        .map(|ch| ch.to_uppercase().collect::<String>())
        .unwrap_or_default();
    let rest: String = chars.collect();
    truncate_chars(&format!("{first}{rest}"), 80)
}

fn draft_id(goal: &str, run_id: &str) -> String {
    let slug = slugify(&redact_text(goal));
    let suffix = run_id
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .take(8)
        .collect::<String>()
        .to_ascii_lowercase();
    let suffix = if suffix.is_empty() {
        "run".to_string()
    } else {
        suffix
    };
    if slug.is_empty() {
        format!("learned-run-{suffix}")
    } else {
        format!("learned-{}-{suffix}", truncate_chars(&slug, 55))
    }
}

fn slugify(text: &str) -> String {
    let mut out = String::new();
    let mut pending_dash = false;
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(ch.to_ascii_lowercase());
        } else {
            pending_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

#[allow(clippy::too_many_arguments)]
fn build_content(
    name: &str,
    description: &str,
    example_prompt: &str,
    run_id: &str,
    bot_name: &str,
    steps: &[LearnedStep],
    tools: &[String],
    warnings: &[String],
    risky: &[String],
    omitted_any: bool,
) -> String {
    let mut body = String::new();
    body.push_str("---\n");
    body.push_str(&format!("name: {}\n", yaml_string(name)));
    body.push_str(&format!("description: {}\n", yaml_string(description)));
    body.push_str(&format!(
        "example-prompt: {}\n",
        yaml_string(example_prompt)
    ));
    body.push_str("---\n\n");

    body.push_str(&format!("# {name}\n\n"));
    body.push_str("## When to use\n\n");
    if example_prompt.trim().is_empty() {
        body.push_str("Use this skill to repeat the workflow recorded in the source run.\n\n");
    } else {
        body.push_str(&format!(
            "Use this skill when the task is: {example_prompt}\n\n"
        ));
    }
    body.push_str(
        "This is a reviewed draft learned from one completed RAVENBOT run. It records the \
         tool sequence, not the model's reasoning or the run's output.\n\n",
    );

    body.push_str("## Steps\n\n");
    for (index, step) in steps.iter().enumerate() {
        body.push_str(&format!(
            "{}. **{}** — {}\n",
            index + 1,
            inline_markdown(&step.tool),
            inline_markdown(&step.summary)
        ));
    }
    body.push('\n');

    body.push_str("## Tools\n\n");
    for tool in tools {
        let count = steps.iter().filter(|step| &step.tool == tool).count();
        body.push_str(&format!(
            "- `{}` — used in {count} step(s).\n",
            inline_markdown(tool)
        ));
    }
    body.push('\n');

    body.push_str("## Verification\n\n");
    body.push_str(
        "- Run the steps again in the same environment and check the task's own success criteria.\n",
    );
    body.push_str(
        "- This draft contains no test results. RAVENBOT did not re-run the task when it wrote \
         this document, so verify the outcome yourself.\n\n",
    );

    body.push_str("## Evidence\n\n");
    body.push_str(&format!("- Source run: `{}`\n", inline_markdown(run_id)));
    body.push_str(&format!("- Source agent: {}\n", inline_markdown(bot_name)));
    body.push_str(
        "- Derived only from the run's successful tool calls. Failed calls and raw tool output \
         are excluded.\n\n",
    );

    body.push_str("## Safety\n\n");
    body.push_str(
        "- Review every step before running it. Saving a skill does not grant permissions and \
         does not bypass approvals.\n",
    );
    if risky.is_empty() {
        body.push_str(
            "- No high-risk tool was detected by the draft generator; this is not a guarantee.\n",
        );
    } else {
        body.push_str(&format!(
            "- High-risk tools detected: {}. Each run still follows the approval policy.\n",
            risky.join(", ")
        ));
    }
    if omitted_any {
        body.push_str(
            "- Typed text, file contents, request bodies and scripts are omitted; re-enter any \
             values the task needs.\n",
        );
    }
    if !warnings.is_empty() {
        body.push_str("\n### Draft warnings\n\n");
        for warning in warnings {
            body.push_str(&format!("- {}\n", inline_markdown(warning)));
        }
    }
    body
}

/// JSON string escaping is valid YAML for a double-quoted scalar, including
/// quotes, backslashes and newlines.
fn yaml_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

/// One line, no angle brackets from `<think>`-style markup, no backticks that
/// could break an inline-code span. Newlines are already collapsed by
/// `clean_inline`.
fn inline_markdown(value: &str) -> String {
    value.replace('`', "'").replace(['\n', '\r'], " ")
}

fn clean_inline(value: &str, max_chars: usize) -> String {
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    truncate_chars(collapsed.trim(), max_chars)
}

fn clean_selector(value: &str) -> String {
    let cleaned = clean_inline(value, 120).replace('`', "'");
    if cleaned.is_empty() {
        String::new()
    } else {
        format!("`{cleaned}`")
    }
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if out.chars().count() >= max_chars {
            break;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(name: &str, arguments: Value) -> ToolTrace {
        ToolTrace::new(name, arguments).with_result(json!({ "ok": true }), false)
    }

    fn failed(name: &str, arguments: Value) -> ToolTrace {
        ToolTrace::new(name, arguments).with_result(json!({ "error": "boom" }), true)
    }

    fn draft_text(draft: &SkillDraft) -> String {
        format!(
            "{}\n{}\n{}",
            serde_json::to_string(&draft).expect("serialize draft"),
            serde_json::to_string(&draft.steps).expect("serialize steps"),
            draft.content
        )
    }

    #[test]
    fn failed_calls_are_left_out_and_named_in_warnings() {
        let traces = vec![
            ok("shell_exec", json!({ "command": "cargo test" })),
            failed("file_write", json!({ "path": "/tmp/nope" })),
            ok("file_read", json!({ "path": "src/lib.rs" })),
        ];
        let draft =
            propose_skill_from_traces("fix the build", "Atlas", "run-1", &traces).expect("draft");
        assert_eq!(draft.steps.len(), 2);
        assert_eq!(draft.steps[0].tool, "shell_exec");
        assert_eq!(draft.steps[1].tool, "file_read");
        assert!(draft
            .warnings
            .iter()
            .any(|warning| warning.contains("failed") && warning.contains("file_write")));
        assert!(!draft.content.contains("boom"));
    }

    #[test]
    fn consecutive_identical_calls_are_collapsed() {
        let traces = vec![
            ok("shell_exec", json!({ "command": "cargo test" })),
            ok("shell_exec", json!({ "command": "cargo test" })),
            ok("file_read", json!({ "path": "src/lib.rs" })),
        ];
        let draft =
            propose_skill_from_traces("fix the build", "Atlas", "run-1", &traces).expect("draft");
        assert_eq!(draft.steps.len(), 2);
        assert!(draft
            .warnings
            .iter()
            .any(|warning| warning.contains("collapsed")));
    }

    #[test]
    fn a_browser_sequence_becomes_one_readable_step() {
        let traces = vec![
            ok("browser_dom", json!({ "action": "snapshot" })),
            ok(
                "browser_dom",
                json!({ "action": "click", "selector": "#sign-in" }),
            ),
            ok(
                "browser_dom",
                json!({ "action": "fill", "selector": "#email", "text": "person@example.com" }),
            ),
            ok(
                "browser_dom",
                json!({ "action": "press", "key": "Enter", "modifiers": ["Ctrl"] }),
            ),
        ];
        let draft = propose_skill_from_traces("sign in", "Atlas", "run-1", &traces).expect("draft");
        assert_eq!(draft.steps.len(), 1);
        let step = &draft.steps[0];
        assert_eq!(step.tool, "browser_dom");
        assert_eq!(
            step.action.as_deref(),
            Some("snapshot → click → fill → press")
        );
        assert!(
            step.summary.contains("Click `#sign-in`"),
            "{}",
            step.summary
        );
        assert!(step.summary.contains("Fill `#email`"), "{}", step.summary);
        assert!(
            step.summary.contains("Press Ctrl+Enter"),
            "{}",
            step.summary
        );
        assert!(!step.summary.contains("person@example.com"));
        assert_eq!(step.arguments["actions"].as_array().map(Vec::len), Some(4));
        assert!(!serde_json::to_string(step)
            .unwrap()
            .contains("person@example.com"));
    }

    #[test]
    fn desktop_actions_group_into_one_step() {
        let traces = vec![
            ok(
                "computer_control",
                json!({ "action": "click", "x": 10, "y": 20 }),
            ),
            ok(
                "computer_control",
                json!({ "action": "type", "text": "secret typed text" }),
            ),
            ok(
                "computer_control",
                json!({ "action": "key", "key": "Enter" }),
            ),
        ];
        let draft =
            propose_skill_from_traces("submit the form", "Atlas", "run-1", &traces).expect("draft");
        assert_eq!(draft.steps.len(), 1);
        let step = &draft.steps[0];
        assert_eq!(step.action.as_deref(), Some("click → type → key"));
        assert!(step.summary.contains("Click at (10, 20)"));
        assert!(step.summary.contains("text omitted"));
        assert!(!serde_json::to_string(step)
            .unwrap()
            .contains("secret typed text"));
    }

    #[test]
    fn a_run_with_only_failed_calls_is_refused() {
        let traces = vec![failed("shell_exec", json!({ "command": "false" }))];
        let error =
            propose_skill_from_traces("try", "Atlas", "run-1", &traces).expect_err("no happy path");
        assert!(error.contains("failed"), "{error}");
    }

    #[test]
    fn an_empty_trace_is_refused() {
        let error = propose_skill_from_traces("try", "Atlas", "run-1", &[]).expect_err("empty");
        assert!(error.contains("no tool trace"), "{error}");
    }

    #[test]
    fn steps_are_capped_with_a_warning() {
        let traces = (0..(MAX_LEARNED_STEPS + 7))
            .map(|index| ok("shell_exec", json!({ "command": format!("echo {index}") })))
            .collect::<Vec<_>>();
        let draft = propose_skill_from_traces("loop", "Atlas", "run-1", &traces).expect("draft");
        assert_eq!(draft.steps.len(), MAX_LEARNED_STEPS);
        assert!(draft
            .warnings
            .iter()
            .any(|warning| warning.contains("7 later step")));
    }

    #[test]
    fn planted_secrets_never_reach_any_part_of_the_draft() {
        let secrets = [
            "sk-live-abcdefghijklmnop",
            "ghp_abcdefghijklmnopqrstuvwxyz",
            "xoxb-1234567890-abcdefgh",
            "AKIAIOSFODNN7EXAMPLE",
            "hunter2-super-secret",
            "Bearer-token-value-123456789",
            "nested-secret-value",
            "cookie-session-value",
        ];
        let traces = vec![
            ok(
                "shell_exec",
                json!({
                    "command": "curl -H 'Authorization: Bearer Bearer-token-value-123456789' https://example.com",
                    "password": "hunter2-super-secret",
                    "env": {
                        "OPENAI_API_KEY": "sk-live-abcdefghijklmnop",
                        "GITHUB_TOKEN": "ghp_abcdefghijklmnopqrstuvwxyz",
                        "SLACK_TOKEN": "xoxb-1234567890-abcdefgh",
                        "AWS_ACCESS_KEY_ID": "AKIAIOSFODNN7EXAMPLE",
                        "SESSION_ID": "nested-secret-value",
                    },
                    "cookie": "cookie-session-value",
                }),
            )
            .with_result(
                json!({ "stdout": "sk-live-abcdefghijklmnop ghp_abcdefghijklmnopqrstuvwxyz" }),
                false,
            ),
            ok(
                "file_write",
                json!({
                    "path": "/tmp/notes.md",
                    "content": "password=hunter2-super-secret",
                }),
            ),
        ];
        let draft = propose_skill_from_traces(
            "use sk-live-abcdefghijklmnop to finish",
            "Atlas",
            "run-1",
            &traces,
        )
        .expect("draft");
        let haystack = draft_text(&draft);
        for secret in secrets {
            assert!(
                !haystack.contains(secret),
                "secret {secret} leaked into the draft: {haystack}"
            );
        }
        assert!(haystack.contains(REDACTED));
        assert!(!haystack.contains("Bearer-token-value-123456789"));
        assert!(draft
            .warnings
            .iter()
            .any(|warning| warning.contains("credentials")));
    }

    #[test]
    fn sensitive_keys_are_redacted_recursively() {
        let value = json!({
            "safe": "kept",
            "password": "one",
            "nested": [{
                "api_key": "two",
                "authorization": "three",
                "cookie": "four",
                "private_key": "five",
                "session_id": "six",
                "credentials": { "value": "seven" }
            }]
        });
        let redacted = redact_json(&value);
        assert_eq!(redacted["safe"], "kept");
        assert_eq!(redacted["password"], REDACTED);
        assert_eq!(redacted["nested"][0]["api_key"], REDACTED);
        assert_eq!(redacted["nested"][0]["authorization"], REDACTED);
        assert_eq!(redacted["nested"][0]["cookie"], REDACTED);
        assert_eq!(redacted["nested"][0]["private_key"], REDACTED);
        assert_eq!(redacted["nested"][0]["session_id"], REDACTED);
        assert_eq!(redacted["nested"][0]["credentials"], REDACTED);
    }

    #[test]
    fn credential_shapes_are_redacted_in_free_text() {
        let text = "Use sk-abcdefghijklmnop, ghp_abcdefghijklmnopqrstuvwxyz, \
                    xoxp-1234567890-abcdefgh, xoxb-1234567890-abcdefgh, \
                    AKIAIOSFODNN7EXAMPLE, Bearer abcdefghijklmnop and \
                    API_KEY=abcdefghijklmnop";
        let redacted = redact_text(text);
        for secret in [
            "sk-abcdefghijklmnop",
            "ghp_abcdefghijklmnopqrstuvwxyz",
            "xoxp-1234567890-abcdefgh",
            "xoxb-1234567890-abcdefgh",
            "AKIAIOSFODNN7EXAMPLE",
            "abcdefghijklmnop",
        ] {
            assert!(!redacted.contains(secret), "{secret} survived: {redacted}");
        }
        assert!(redacted.contains("Bearer [redacted]"), "{redacted}");
        assert!(redacted.contains("API_KEY=[redacted]"), "{redacted}");
    }

    #[test]
    fn high_risk_tools_are_called_out() {
        let traces = vec![
            ok("shell_exec", json!({ "command": "cargo test" })),
            ok(
                "browser_dom",
                json!({ "action": "click", "selector": "#submit" }),
            ),
        ];
        let draft = propose_skill_from_traces("submit", "Atlas", "run-1", &traces).expect("draft");
        let warning = draft
            .warnings
            .iter()
            .find(|warning| warning.contains("high-risk"))
            .expect("high-risk warning");
        assert!(warning.contains("shell_exec"), "{warning}");
        assert!(warning.contains("browser_dom (click)"), "{warning}");
        assert!(draft.content.contains("High-risk tools detected"));
    }

    #[test]
    fn the_document_has_valid_frontmatter_and_required_sections() {
        let traces = vec![ok("shell_exec", json!({ "command": "cargo test" }))];
        let draft =
            propose_skill_from_traces("run the tests", "Atlas", "run-42", &traces).expect("draft");
        assert!(draft.content.starts_with("---\n"));
        for expected in [
            "name: ",
            "description: ",
            "example-prompt: ",
            "## When to use",
            "## Steps",
            "## Tools",
            "## Verification",
            "## Evidence",
            "## Safety",
            "run-42",
            "Atlas",
        ] {
            assert!(
                draft.content.contains(expected),
                "missing {expected} in {}",
                draft.content
            );
        }
        // The frontmatter must parse as YAML and agree with the draft fields.
        let rest = draft
            .content
            .strip_prefix("---\n")
            .expect("frontmatter opens");
        let end = rest.find("\n---").expect("frontmatter closes");
        let header: serde_yaml::Value =
            serde_yaml::from_str(&rest[..end]).expect("valid frontmatter YAML");
        assert_eq!(header["name"].as_str(), Some(draft.name.as_str()));
        assert_eq!(
            header["description"].as_str(),
            Some(draft.description.as_str())
        );
        assert_eq!(
            header["example-prompt"].as_str(),
            Some(draft.example_prompt.as_str())
        );

        // The draft must not claim a test ran.
        assert!(!draft.content.contains("tests passed"));
    }

    #[test]
    fn a_non_ascii_goal_keeps_a_readable_name_and_a_safe_id() {
        let traces = vec![ok("shell_exec", json!({ "command": "echo hi" }))];
        let draft = propose_skill_from_traces("請求書を作成する", "Atlas", "run-1", &traces)
            .expect("draft");
        assert!(draft.name.contains("請求書"));
        assert!(draft.id.starts_with("learned-run-"), "{}", draft.id);
        assert!(draft
            .id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.')));
    }

    #[test]
    fn nested_payload_fields_are_stripped_and_write_like_mcp_is_flagged() {
        let traces = vec![ok(
            "mcp__crm__create_note",
            json!({
                "record": {
                    "id": "42",
                    "text": "private note body"
                },
                "content": "another private body"
            }),
        )];
        let draft =
            propose_skill_from_traces("save a note", "Atlas", "run-1", &traces).expect("draft");
        let serialized = serde_json::to_string(&draft.steps).unwrap();
        assert!(!serialized.contains("private note body"), "{serialized}");
        assert!(!serialized.contains("another private body"), "{serialized}");
        assert!(draft
            .warnings
            .iter()
            .any(|warning| warning.contains("mcp__crm__create_note")));
    }

    #[test]
    fn large_payloads_are_omitted_rather_than_truncated_into_nonsense() {
        let big = "x".repeat(10_000);
        let traces = vec![ok(
            "file_write",
            json!({ "path": "big.txt", "content": big }),
        )];
        let draft =
            propose_skill_from_traces("write a file", "Atlas", "run-1", &traces).expect("draft");
        let serialized = serde_json::to_string(&draft.steps).unwrap();
        assert!(!serialized.contains(&"x".repeat(100)));
        assert!(draft
            .warnings
            .iter()
            .any(|warning| warning.contains("omitted")));
    }
}

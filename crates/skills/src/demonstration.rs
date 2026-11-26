//! Learned skills from a person's browser demonstration.
//!
//! A demonstration is a small, deterministic recording of DOM events: a click,
//! a typed value, one of three navigation keys, and a page URL. It is captured
//! in the remote page by [`DEMO_RECORDER_JS`], normalised here, reviewed by the
//! person, and then saved as an ordinary versioned skill with a `demo.json`
//! companion file.
//!
//! Replay is intentionally conservative:
//!
//! - a recorded selector is tried first, then its recorded fallbacks, then a
//!   human label (`aria-label`, `name`, `<label>` text, visible text);
//! - a fallback match is counted as a repair and reported, but never written
//!   back to the skill;
//! - a sensitive field is a manual step, never an empty fill;
//! - a failed step stops the replay and returns a concrete repair hint.
//!
//! The recorder runs in the page origin and never sees the app's IPC bridge.
//! Nothing here calls a model: the same events produce the same draft.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;

use crate::learn::{redact_text, LearnedStep, SkillDraft};

/// A demonstration format version. Replay refuses a version it does not know.
pub const DEMO_VERSION: u32 = 1;
/// Most actions one demonstration may carry.
pub const MAX_DEMO_ACTIONS: usize = 200;
/// Longest recording, in milliseconds.
pub const MAX_DEMO_MS: u64 = 10 * 60 * 1000;
/// Longest selector we accept from the page.
pub const MAX_SELECTOR_CHARS: usize = 200;
/// Most fallback selectors stored after the primary selector.
pub const MAX_DEMO_FALLBACKS: usize = 3;
/// Longest typed value stored for a non-sensitive field.
pub const MAX_DEMO_TEXT_CHARS: usize = 2_000;
/// Companion file written next to `SKILL.md`.
pub const DEMO_FILE: &str = "demo.json";

/// The recorder asset. Rust injects it with `include_str!`; Vitest reads the
/// same file, so the tested JavaScript and the shipped JavaScript are identical.
pub const DEMO_RECORDER_JS: &str = include_str!("../assets/demo-recorder.js");
/// The replay asset. It returns the object consumed by [`parse_replay_step`].
pub const DEMO_REPLAY_JS: &str = include_str!("../assets/demo-replay.js");

/// One reviewed browser demonstration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Demonstration {
    /// Format version. The current version is [`DEMO_VERSION`].
    pub version: u32,
    /// The page the recording started on.
    pub start_url: String,
    /// RFC 3339 timestamp of the moment the person pressed Teach.
    pub recorded_at: String,
    /// The normalised actions in order.
    pub actions: Vec<DemoAction>,
}

/// One action observed in the page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DemoAction {
    /// `navigate`, `click`, `fill`, or `press`.
    pub kind: String,
    /// A navigate target, or the page URL that gives the action context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The primary CSS selector recorded from the page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    /// Additional selectors tried when the primary no longer matches.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fallbacks: Vec<String>,
    /// A human label used for the final fallback match and the review screen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Typed text for a non-sensitive fill. Always absent for a sensitive field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// `Enter`, `Escape`, or `Tab` for a press.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// The value was deliberately not recorded; replay asks the person instead.
    #[serde(default)]
    pub sensitive: bool,
    /// Checkbox or radio state observed after the click.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
}

/// A raw event exactly as the page recorder emitted it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RawDemoEvent {
    /// Unique within a page load. Rust uses it to drop a duplicate that a
    /// re-injected recorder or a final stop() returns.
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub at: i64,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub selector: Option<String>,
    #[serde(default)]
    pub fallbacks: Vec<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub sensitive: bool,
    #[serde(default)]
    pub checked: Option<bool>,
}

/// The result of normalising a raw recording.
#[derive(Debug, Clone, Default)]
pub struct DemoBuild {
    pub demonstration: Demonstration,
    /// Problems the reviewer must see, in addition to draft warnings.
    pub warnings: Vec<String>,
    /// The 200-action cap was reached.
    pub truncated: bool,
    /// Sensitive fill actions included in the demonstration.
    pub sensitive_fields: usize,
}

impl Default for Demonstration {
    fn default() -> Self {
        Self {
            version: DEMO_VERSION,
            start_url: String::new(),
            recorded_at: String::new(),
            actions: Vec::new(),
        }
    }
}

/// Build a demonstration from raw page events.
///
/// Events are ordered by their page timestamp, duplicate ids are dropped, a
/// URL change becomes a `navigate` action, and a typing burst for one field
/// collapses to the final value. Sensitive text is removed before the action
/// exists, so it cannot leak into a draft or `demo.json` later.
pub fn build_demonstration(
    events: &[RawDemoEvent],
    start_url: &str,
    recorded_at: &str,
) -> DemoBuild {
    let start_url = clean_url(start_url).unwrap_or_default();
    let recorded_at = clean_inline(&redact_text(recorded_at), 80);
    let mut warnings = Vec::new();
    let mut seen = HashSet::new();
    let mut ordered: Vec<&RawDemoEvent> = Vec::new();

    for event in events {
        if let Some(id) = event
            .id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
        {
            if !seen.insert(id.to_string()) {
                continue;
            }
        }
        ordered.push(event);
    }
    // Page timestamps are milliseconds. A stable sort keeps equal events in
    // the order the recorder captured them.
    ordered.sort_by_key(|event| event.at);

    let mut actions: Vec<DemoAction> = Vec::new();
    let mut last_url = start_url.clone();
    let mut last_fill: Option<(String, usize, i64)> = None;
    let mut truncated = false;
    let mut dropped_press = 0usize;
    let mut redacted_any = false;

    push_navigate(&mut actions, &start_url, &mut last_url, &mut truncated);

    for event in ordered {
        if truncated {
            break;
        }
        let kind = event.kind.trim().to_ascii_lowercase();
        let event_url = event
            .url
            .as_deref()
            .and_then(clean_url)
            .unwrap_or_else(|| last_url.clone());
        if !event_url.is_empty() && event_url != last_url && kind != "navigate" {
            push_navigate(&mut actions, &event_url, &mut last_url, &mut truncated);
        }

        match kind.as_str() {
            "navigate" => {
                push_navigate(&mut actions, &event_url, &mut last_url, &mut truncated);
                last_fill = None;
            }
            "click" | "fill" | "press" => {
                if kind == "press" && !allowed_key(event.key.as_deref()) {
                    dropped_press += 1;
                    continue;
                }
                let raw_text = event.text.clone();
                let action = match sanitize_action(DemoAction {
                    kind: kind.clone(),
                    url: if event_url.is_empty() {
                        None
                    } else {
                        Some(event_url)
                    },
                    selector: event.selector.clone(),
                    fallbacks: event.fallbacks.clone(),
                    label: event.label.clone(),
                    text: event.text.clone(),
                    key: event.key.clone(),
                    sensitive: event.sensitive,
                    checked: event.checked,
                }) {
                    Some(action) => action,
                    None => continue,
                };
                if action.kind == "fill" {
                    let key = format!(
                        "{}|{}",
                        action.selector.as_deref().unwrap_or_default(),
                        action.url.as_deref().unwrap_or_default()
                    );
                    if let Some((previous_key, index, at)) = &last_fill {
                        if previous_key == &key && event.at.saturating_sub(*at) <= 3_000 {
                            if let Some(previous) = actions.get_mut(*index) {
                                if previous.kind == "fill" {
                                    redacted_any |= !event.sensitive
                                        && raw_text
                                            .as_deref()
                                            .map(|text| text_was_redacted(&action, text))
                                            .unwrap_or(false);
                                    previous.text = action.text.clone();
                                    previous.label = action.label.clone();
                                    previous.sensitive = action.sensitive;
                                    previous.selector = action.selector.clone();
                                    previous.fallbacks = action.fallbacks.clone();
                                    last_fill = Some((key, *index, event.at));
                                    continue;
                                }
                            }
                        }
                    }
                }
                if actions.len() >= MAX_DEMO_ACTIONS {
                    truncated = true;
                    break;
                }
                let index = actions.len();
                redacted_any |= !event.sensitive
                    && raw_text
                        .as_deref()
                        .map(|text| text_was_redacted(&action, text))
                        .unwrap_or(false);
                actions.push(action);
                last_fill = if kind == "fill" {
                    Some((
                        format!(
                            "{}|{}",
                            actions[index].selector.as_deref().unwrap_or_default(),
                            actions[index].url.as_deref().unwrap_or_default()
                        ),
                        index,
                        event.at,
                    ))
                } else {
                    None
                };
            }
            _ => {}
        }
    }

    if actions.len() > MAX_DEMO_ACTIONS {
        actions.truncate(MAX_DEMO_ACTIONS);
        truncated = true;
    }

    if truncated {
        warnings.push(format!(
            "The recording reached the {MAX_DEMO_ACTIONS}-action limit; later actions were not captured."
        ));
    }
    if dropped_press > 0 {
        warnings.push(format!(
            "{dropped_press} key press(es) other than Enter, Escape, or Tab were not recorded."
        ));
    }
    if redacted_any {
        warnings.push(
            "Some recorded values looked like credentials and were replaced with [redacted]."
                .to_string(),
        );
    }
    let sensitive_fields = actions
        .iter()
        .filter(|action| action.kind == "fill" && action.sensitive)
        .count();
    if sensitive_fields > 0 {
        warnings.push(format!(
            "{sensitive_fields} sensitive field(s) were recorded without their values; replay stops and asks you to fill them manually."
        ));
    }

    DemoBuild {
        demonstration: Demonstration {
            version: DEMO_VERSION,
            start_url,
            recorded_at,
            actions,
        },
        warnings,
        truncated,
        sensitive_fields,
    }
}

fn push_navigate(
    actions: &mut Vec<DemoAction>,
    url: &str,
    last_url: &mut String,
    truncated: &mut bool,
) {
    let Some(url) = clean_url(url) else {
        return;
    };
    if actions.len() >= MAX_DEMO_ACTIONS {
        *truncated = true;
        return;
    }
    if actions
        .last()
        .map(|action| action.kind == "navigate" && action.url.as_deref() == Some(url.as_str()))
        .unwrap_or(false)
    {
        *last_url = url;
        return;
    }
    actions.push(DemoAction {
        kind: "navigate".to_string(),
        url: Some(url.clone()),
        selector: None,
        fallbacks: Vec::new(),
        label: None,
        text: None,
        key: None,
        sensitive: false,
        checked: None,
    });
    *last_url = url;
}

/// Re-apply every limit and secret rule to a demonstration that came back from
/// the UI. The review screen only edits `SKILL.md`, but this is the boundary
/// that writes `demo.json`; it must not trust its input.
pub fn sanitize_demonstration(demo: &Demonstration) -> Demonstration {
    let mut actions = Vec::new();
    for action in &demo.actions {
        if actions.len() >= MAX_DEMO_ACTIONS {
            break;
        }
        if let Some(action) = sanitize_action(action.clone()) {
            actions.push(action);
        }
    }
    Demonstration {
        version: DEMO_VERSION,
        start_url: clean_url(&demo.start_url).unwrap_or_default(),
        recorded_at: clean_inline(&redact_text(&demo.recorded_at), 80),
        actions,
    }
}

fn sanitize_action(mut action: DemoAction) -> Option<DemoAction> {
    action.kind = action.kind.trim().to_ascii_lowercase();
    action.url = action.url.as_deref().and_then(clean_url);
    action.selector = action.selector.as_deref().and_then(clean_selector);
    action.label = action
        .label
        .as_deref()
        .map(|label| redact_text(&clean_inline(label, 160)))
        .filter(|label| !label.is_empty());
    let mut fallbacks = Vec::new();
    for fallback in &action.fallbacks {
        if let Some(fallback) = clean_selector(fallback) {
            if Some(&fallback) != action.selector.as_ref() && !fallbacks.contains(&fallback) {
                fallbacks.push(fallback);
                if fallbacks.len() >= MAX_DEMO_FALLBACKS {
                    break;
                }
            }
        }
    }
    action.fallbacks = fallbacks;

    match action.kind.as_str() {
        "navigate" => {
            action.url.as_ref()?;
            action.selector = None;
            action.fallbacks.clear();
            action.label = None;
            action.text = None;
            action.key = None;
            action.sensitive = false;
            action.checked = None;
        }
        "click" => {
            action.text = None;
            action.key = None;
            if action.selector.is_none() && action.label.is_none() {
                return None;
            }
        }
        "fill" => {
            action.key = None;
            if action.selector.is_none() && action.label.is_none() {
                return None;
            }
            if action.sensitive {
                action.text = None;
            } else {
                action.text = action
                    .text
                    .map(|text| redact_text(&truncate_chars(&text, MAX_DEMO_TEXT_CHARS)));
            }
        }
        "press" => {
            action.text = None;
            if !allowed_key(action.key.as_deref()) {
                return None;
            }
            action.sensitive = false;
            action.checked = None;
        }
        _ => return None,
    }
    Some(action)
}

fn allowed_key(key: Option<&str>) -> bool {
    matches!(key.map(str::trim), Some("Enter" | "Escape" | "Tab"))
}

fn text_was_redacted(action: &DemoAction, raw: &str) -> bool {
    match action.text.as_deref() {
        Some(cleaned) => cleaned != redact_text(&truncate_chars(raw, MAX_DEMO_TEXT_CHARS)),
        None => true,
    }
}

fn clean_selector(raw: &str) -> Option<String> {
    let selector = raw.trim();
    if selector.is_empty()
        || selector.chars().count() > MAX_SELECTOR_CHARS
        || selector.chars().any(char::is_control)
    {
        return None;
    }
    Some(selector.to_string())
}

fn clean_url(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() || raw.len() > 4_096 {
        return None;
    }
    let parsed = url::Url::parse(raw).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    Some(redact_text(parsed.as_str()))
}

fn clean_inline(value: &str, max_chars: usize) -> String {
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    truncate_chars(collapsed.trim(), max_chars)
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn host_of(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_string))
        .map(|host| host.trim_start_matches("www.").to_string())
        .filter(|host| !host.is_empty())
        .unwrap_or_else(|| "browser".to_string())
}

fn demo_id(host: &str, recorded_at: &str) -> String {
    let host_slug = slug(host);
    let stamp: String = recorded_at
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .take(15)
        .collect::<String>()
        .to_ascii_lowercase();
    let stamp = if stamp.is_empty() {
        "recorded".to_string()
    } else {
        stamp
    };
    if host_slug.is_empty() {
        format!("demo-browser-{stamp}")
    } else {
        format!("demo-{}-{}", truncate_chars(&host_slug, 55), stamp)
    }
}

fn slug(value: &str) -> String {
    let mut out = String::new();
    let mut pending_dash = false;
    for ch in value.chars() {
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

fn action_label(action: &DemoAction) -> String {
    action
        .label
        .as_deref()
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map(str::to_string)
        .or_else(|| action.selector.clone())
        .or_else(|| action.url.clone())
        .unwrap_or_else(|| "the page".to_string())
}

/// A short, redacted description of one action for the review screen and the
/// replay progress line.
pub fn summarize_action(action: &DemoAction) -> String {
    match action.kind.as_str() {
        "navigate" => format!(
            "Open {}",
            action.url.as_deref().unwrap_or("the recorded page")
        ),
        "click" => format!("Click {}", action_label(action)),
        "fill" => {
            if action.sensitive {
                format!("Fill {} (needs your input)", action_label(action))
            } else {
                match action.text.as_deref() {
                    Some(text) if text.is_empty() => {
                        format!("Clear {}", action_label(action))
                    }
                    Some(text) => format!(
                        "Fill {} with \"{}\"",
                        action_label(action),
                        truncate_chars(&redact_text(&text.replace(['\n', '\r'], " ")), 80)
                    ),
                    None => format!("Fill {} (value omitted)", action_label(action)),
                }
            }
        }
        "press" => format!(
            "Press {} on {}",
            action.key.as_deref().unwrap_or("a key"),
            action_label(action)
        ),
        _ => format!("Run {}", action.kind),
    }
}

/// Turn a demonstration into a reviewed skill draft. Deterministic: no model,
/// no re-run, no verification claim.
pub fn propose_skill_from_demonstration(
    demo: &Demonstration,
    bot_name: &str,
) -> Result<SkillDraft, String> {
    if demo.version != DEMO_VERSION {
        return Err(format!(
            "This demonstration uses format version {}, but this build understands version {DEMO_VERSION}.",
            demo.version
        ));
    }
    let demo = sanitize_demonstration(demo);
    if demo.actions.is_empty() {
        return Err("This demonstration has no recordable browser actions.".to_string());
    }
    if demo.start_url.is_empty() {
        return Err("This demonstration has no usable start URL.".to_string());
    }

    let bot_name = redact_text(&clean_inline(bot_name, 80));
    let bot_name = if bot_name.is_empty() {
        "you".to_string()
    } else {
        bot_name
    };
    let host = host_of(&demo.start_url);
    let name = truncate_chars(&format!("Demonstrated {host} workflow"), 80);
    let id = demo_id(&host, &demo.recorded_at);
    let description = truncate_chars(
        &format!(
            "A reviewed browser workflow demonstrated on {host} with {} action(s).",
            demo.actions.len()
        ),
        240,
    );
    let example_prompt = format!("Replay the browser workflow demonstrated on {host}.");

    let mut steps = Vec::with_capacity(demo.actions.len());
    for action in &demo.actions {
        let step = match action.kind.as_str() {
            "navigate" => LearnedStep {
                tool: "browser_open".to_string(),
                action: Some("open".to_string()),
                summary: redact_text(&summarize_action(action)),
                arguments: json!({
                    "action": "open",
                    "url": action.url,
                }),
            },
            _ => {
                let mut arguments = json!({
                    "action": action.kind,
                    "selector": action.selector,
                    "fallbacks": action.fallbacks,
                    "label": action.label,
                    "sensitive": action.sensitive,
                });
                if action.kind == "fill" && !action.sensitive {
                    if let Some(text) = &action.text {
                        arguments["text"] = json!(text);
                    }
                }
                if action.kind == "press" {
                    arguments["key"] = json!(action.key);
                }
                if let Some(checked) = action.checked {
                    arguments["checked"] = json!(checked);
                }
                LearnedStep {
                    tool: "browser_dom".to_string(),
                    action: Some(action.kind.clone()),
                    summary: redact_text(&summarize_action(action)),
                    arguments,
                }
            }
        };
        steps.push(step);
    }

    let tools = {
        let mut tools = Vec::new();
        for step in &steps {
            if !tools.contains(&step.tool) {
                tools.push(step.tool.clone());
            }
        }
        tools
    };

    let sensitive_fields = demo
        .actions
        .iter()
        .filter(|action| action.kind == "fill" && action.sensitive)
        .count();
    let has_press = demo.actions.iter().any(|action| action.kind == "press");
    let has_fallback = demo
        .actions
        .iter()
        .any(|action| !action.fallbacks.is_empty());
    let mut warnings = vec![
        "This draft records a demonstration and was not verified by re-running it. Check the outcome after replay."
            .to_string(),
        "Replay tries the recorded selector, its fallbacks, then the recorded label. A repaired step is reported but the skill is never rewritten automatically."
            .to_string(),
    ];
    if sensitive_fields > 0 {
        warnings.push(format!(
            "{sensitive_fields} sensitive field(s) were not recorded. Replay stops at those steps and asks you to fill them manually."
        ));
    }
    if has_press {
        warnings.push(
            "Press steps dispatch synthetic key events: Enter requests submission of the enclosing form when no handler cancels it and the form is valid; Escape and Tab depend on the page's own handlers."
                .to_string(),
        );
    }
    if !has_fallback {
        warnings.push(
            "No fallback selectors were recorded, so a changed page leaves only the human label to find the control."
                .to_string(),
        );
    }
    if demo.actions.len() >= MAX_DEMO_ACTIONS {
        warnings.push(format!(
            "The recording reached the {MAX_DEMO_ACTIONS}-action limit; the skill may be incomplete."
        ));
    }

    let content = build_content(
        &name,
        &description,
        &example_prompt,
        &demo,
        &steps,
        &tools,
        &warnings,
        sensitive_fields,
        has_press,
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
        source_run_id: format!("demo:{}", demo.recorded_at),
        source_bot_name: bot_name,
        demonstration: Some(demo),
    })
}

fn build_content(
    name: &str,
    description: &str,
    example_prompt: &str,
    demo: &Demonstration,
    steps: &[LearnedStep],
    tools: &[String],
    warnings: &[String],
    sensitive_fields: usize,
    has_press: bool,
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
    body.push_str(&format!(
        "Use this skill when the task is: {example_prompt}\n\n"
    ));
    body.push_str(
        "This is a reviewed draft learned from a person's browser demonstration. It records what \
         was done in the page, not a verified run or the person's reasoning.\n\n",
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

    body.push_str("## Replay\n\n");
    body.push_str(&format!(
        "- Start at {} and run the steps in order from the browser dock.\n",
        inline_markdown(&demo.start_url)
    ));
    body.push_str(
        "- Replay asks for confirmation, tries each recorded selector and its fallbacks, and reports a repair when a fallback matched.\n",
    );
    body.push_str(
        "- A failed selector stops the replay with a repair hint. The skill is not rewritten automatically.\n",
    );
    if sensitive_fields > 0 {
        body.push_str(
            "- Sensitive fields are never filled by replay. It stops and asks you to enter the value in the page.\n",
        );
    }
    if has_press {
        body.push_str(
            "- Press steps dispatch synthetic key events. Enter requests submission of the enclosing form unless a handler cancels it and the form is valid; Escape and Tab rely on the page's own handlers.\n",
        );
    }
    body.push('\n');

    body.push_str("## Verification\n\n");
    body.push_str(
        "- This draft contains no test results. RAVENBOT did not re-run the demonstration when it wrote this document.\n",
    );
    body.push_str(
        "- After a replay, check the task's own success criteria; a completed replay is not proof that the page reached the intended state.\n\n",
    );

    body.push_str("## Evidence\n\n");
    body.push_str(&format!(
        "- Demonstrated on {} at {}.\n",
        inline_markdown(&demo.recorded_at),
        inline_markdown(&demo.start_url)
    ));
    body.push_str(
        "- Captured from DOM events in the browser dock by a person; it becomes a skill only after review.\n",
    );
    body.push_str(
        "- Action count: the demonstration carries the observed sequence only; no page output or secret value is included.\n\n",
    );

    body.push_str("## Safety\n\n");
    body.push_str(
        "- Replay clicks and fills a real page. Only start it on a page where those actions are intended.\n",
    );
    body.push_str(
        "- Saving a skill does not grant permissions and does not bypass browser takeover or the approval policy.\n",
    );
    if !warnings.is_empty() {
        body.push_str("\n### Draft warnings\n\n");
        for warning in warnings {
            body.push_str(&format!("- {}\n", inline_markdown(warning)));
        }
    }
    body
}

fn yaml_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

fn inline_markdown(value: &str) -> String {
    value.replace('`', "'").replace(['\n', '\r'], " ")
}

// ── replay contract ───────────────────────────────────────────────────────

/// One action's answer from the page replay helper.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReplayStep {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub matched_by: Option<String>,
    #[serde(default)]
    pub selector: Option<String>,
    #[serde(default)]
    pub page_url: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub attempted: Vec<String>,
    #[serde(default)]
    pub needs_input: bool,
    #[serde(default)]
    pub synthetic: bool,
    #[serde(default)]
    pub submitted_form: bool,
}

/// A selector that was replaced at replay time. The skill is never changed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayRepair {
    pub index: usize,
    pub selector: Option<String>,
    pub used: Option<String>,
    pub matched_by: String,
}

/// The report shown after a replay. Failure is data, not an exception: the UI
/// needs the step, the page and the hint to explain what to fix.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReplayReport {
    pub ok: bool,
    pub completed: usize,
    pub total: usize,
    #[serde(default)]
    pub repaired: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed_index: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<DemoAction>,
    pub page_url: String,
    #[serde(default)]
    pub attempted: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repair_hint: Option<String>,
    #[serde(default)]
    pub repairs: Vec<ReplayRepair>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub needs_input: bool,
}

/// Injection for the first recording or a re-injection after navigation. The
/// asset defines `__ravenbotDemo`; the tiny call starts it with a new buffer
/// and the original recording clock.
pub fn build_recorder_start_script(start_url: &str, started_at_ms: i64, fresh: bool) -> String {
    format!(
        "{DEMO_RECORDER_JS}\n(() => {{ const recorder = window.__ravenbotDemo; if (!recorder) return {{ ok: false, error: 'The demonstration recorder did not load.' }}; return recorder.start({{ fresh: {fresh}, start_url: {url}, started_at: {started_at_ms} }}); }})()",
        url = json_string(start_url),
    )
}

/// Inject the recorder again after a navigation. The new page has a new
/// JavaScript context, so `start` begins a fresh buffer with the original
/// recording clock; Rust already holds the events harvested before the load.
pub fn build_recorder_resume_script(start_url: &str, started_at_ms: i64) -> String {
    build_recorder_start_script(start_url, started_at_ms, false)
}

/// Ask for events after a cursor. Rust stores the cursor per page load.
pub fn build_recorder_status_script(after: usize) -> String {
    format!(
        "(() => {{ const recorder = window.__ravenbotDemo; return recorder ? recorder.status({after}) : {{ ok: false, recording: false, stopped: false, count: 0, events: [] }}; }})()"
    )
}

/// Stop the recorder and return every event. `clear` discards the in-page
/// buffer for the cancel path; nothing is written to page storage.
pub fn build_recorder_stop_script(clear: bool) -> String {
    format!(
        "{DEMO_RECORDER_JS}\n(() => {{ const recorder = window.__ravenbotDemo; return recorder ? recorder.stop({{ clear: {clear} }}) : {{ ok: false, recording: false, stopped: false, count: 0, events: [] }}; }})()"
    )
}

/// Build the page script for one replay action. The asset is included in every
/// call so a navigation cannot leave the page without the helper.
pub fn build_replay_script(action: &DemoAction) -> Result<String, String> {
    let action = serde_json::to_string(action)
        .map_err(|error| format!("Could not serialize the replay action: {error}"))?;
    Ok(format!(
        "{DEMO_REPLAY_JS}\nwindow.__ravenbotReplay.run({action});"
    ))
}

/// Read the helper's object without trusting its shape.
pub fn parse_replay_step(value: &Value) -> ReplayStep {
    serde_json::from_value(value.clone()).unwrap_or_else(|_| ReplayStep {
        ok: false,
        error: Some("The page did not return a usable replay result.".to_string()),
        ..ReplayStep::default()
    })
}

/// A concrete, non-destructive hint for a failed step.
pub fn replay_repair_hint(
    action: &DemoAction,
    page_url: &str,
    attempted: &[String],
    error: &str,
) -> String {
    let label = action_label(action);
    if attempted.is_empty() {
        return format!(
            "Replay stopped before touching this step: {error}. Open {page_url} and run Replay again after fixing the cause. RAVENBOT did not change the skill."
        );
    }
    let tried = attempted
        .iter()
        .map(|value| format!("`{value}`"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "Replay tried {tried} and then the label \"{label}\". Open {page_url}, find the control labelled \"{label}\", and re-record the demonstration or edit demo.json. RAVENBOT stopped with: {error}. It did not change the skill."
    )
}

/// The hint for a sensitive field, which is a manual step rather than a fault.
pub fn sensitive_repair_hint(action: &DemoAction, page_url: &str) -> String {
    let label = action_label(action);
    let selector = action
        .selector
        .as_deref()
        .map(|selector| format!(" (`{selector}`)"))
        .unwrap_or_default();
    format!(
        "This value was deliberately not recorded. In {page_url}, fill \"{label}\"{selector} manually and complete the workflow yourself, or record a demonstration that does not include this sensitive field. Replay always restarts from the beginning."
    )
}

/// The selectors that a replay will try, in order.
pub fn attempted_selectors(action: &DemoAction) -> Vec<String> {
    let mut attempted = Vec::new();
    if let Some(selector) = &action.selector {
        attempted.push(selector.clone());
    }
    for fallback in &action.fallbacks {
        if !attempted.contains(fallback) {
            attempted.push(fallback.clone());
        }
    }
    if let Some(label) = &action.label {
        attempted.push(format!("label:{label}"));
    }
    attempted
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(kind: &str, at: i64, url: &str) -> RawDemoEvent {
        RawDemoEvent {
            id: Some(format!("{kind}-{at}")),
            kind: kind.to_string(),
            at,
            url: Some(url.to_string()),
            ..RawDemoEvent::default()
        }
    }

    fn demo(actions: Vec<DemoAction>) -> Demonstration {
        Demonstration {
            version: 1,
            start_url: "https://example.com/app".to_string(),
            recorded_at: "2026-11-26T10:00:00Z".to_string(),
            actions,
        }
    }

    #[test]
    fn the_start_url_and_url_changes_become_navigate_actions() {
        let mut first = raw("click", 1, "https://example.com/app");
        first.selector = Some("#first".to_string());
        let mut second = raw("click", 2, "https://example.com/next");
        second.selector = Some("#second".to_string());
        let mut third = raw("fill", 3, "https://example.com/next");
        third.selector = Some("#query".to_string());
        third.text = Some("shoes".to_string());
        let build = build_demonstration(
            &[first, second, third],
            "https://example.com/app",
            "2026-11-26T10:00:00Z",
        );
        let kinds = build
            .demonstration
            .actions
            .iter()
            .map(|action| action.kind.as_str())
            .collect::<Vec<_>>();
        assert_eq!(kinds, ["navigate", "click", "navigate", "click", "fill"]);
        assert_eq!(
            build.demonstration.actions[0].url.as_deref(),
            Some("https://example.com/app")
        );
        assert_eq!(
            build.demonstration.actions[2].url.as_deref(),
            Some("https://example.com/next")
        );
    }

    #[test]
    fn a_typing_burst_becomes_one_fill_with_the_final_value() {
        let mut first = raw("fill", 10, "https://example.com/app");
        first.selector = Some("#query".to_string());
        first.text = Some("sh".to_string());
        let mut second = raw("fill", 11, "https://example.com/app");
        second.id = Some("fill-second".to_string());
        second.selector = Some("#query".to_string());
        second.text = Some("shoes".to_string());
        let mut click = raw("click", 12, "https://example.com/app");
        click.selector = Some("#go".to_string());
        click.label = Some("Search".to_string());

        let build = build_demonstration(
            &[first, second, click],
            "https://example.com/app",
            "2026-11-26T10:00:00Z",
        );
        let fills = build
            .demonstration
            .actions
            .iter()
            .filter(|action| action.kind == "fill")
            .collect::<Vec<_>>();
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].text.as_deref(), Some("shoes"));
    }

    #[test]
    fn duplicate_event_ids_are_dropped_across_page_restores() {
        let mut event = raw("click", 5, "https://example.com/app");
        event.selector = Some("#go".to_string());
        let build = build_demonstration(
            &[event.clone(), event],
            "https://example.com/app",
            "2026-11-26T10:00:00Z",
        );
        assert_eq!(
            build
                .demonstration
                .actions
                .iter()
                .filter(|action| action.kind == "click")
                .count(),
            1
        );
    }

    #[test]
    fn the_action_cap_is_enforced_with_a_warning() {
        let events = (0..(MAX_DEMO_ACTIONS + 9))
            .map(|index| {
                let mut event = raw("click", index as i64, "https://example.com/app");
                event.id = Some(format!("click-{index}"));
                event.selector = Some(format!("#button-{index}"));
                event
            })
            .collect::<Vec<_>>();
        let build = build_demonstration(&events, "https://example.com/app", "now");
        assert_eq!(build.demonstration.actions.len(), MAX_DEMO_ACTIONS);
        assert!(build.truncated);
        assert!(build
            .warnings
            .iter()
            .any(|warning| warning.contains("limit")));
    }

    #[test]
    fn sensitive_fills_never_keep_text() {
        let mut event = raw("fill", 1, "https://example.com/login");
        event.selector = Some("#password".to_string());
        event.label = Some("Password".to_string());
        event.text = Some("hunter2-super-secret".to_string());
        event.sensitive = true;
        let build = build_demonstration(
            &[event],
            "https://example.com/login",
            "2026-11-26T10:00:00Z",
        );
        let action = build
            .demonstration
            .actions
            .iter()
            .find(|action| action.kind == "fill")
            .expect("fill");
        assert!(action.sensitive);
        assert!(action.text.is_none());
        assert!(!serde_json::to_string(&build.demonstration)
            .unwrap()
            .contains("hunter2-super-secret"));
        assert!(build
            .warnings
            .iter()
            .any(|warning| warning.contains("sensitive")));
    }

    #[test]
    fn sensitive_text_is_removed_at_the_save_boundary_too() {
        let action = DemoAction {
            kind: "fill".to_string(),
            url: Some("https://example.com/login".to_string()),
            selector: Some("#password".to_string()),
            fallbacks: vec!["/".to_string(), "x".repeat(400)],
            label: Some("Password".to_string()),
            text: Some("never-store-this".to_string()),
            key: None,
            sensitive: true,
            checked: None,
        };
        let sanitized = sanitize_demonstration(&demo(vec![action]));
        assert!(sanitized.actions[0].text.is_none());
        assert!(sanitized.actions[0]
            .fallbacks
            .iter()
            .all(|value| value.chars().count() <= MAX_SELECTOR_CHARS));
    }

    #[test]
    fn selectors_are_validated_and_fallbacks_are_capped() {
        let mut event = raw("click", 1, "https://example.com/app");
        event.selector = Some("#go".to_string());
        event.fallbacks = vec![
            "#go".to_string(),
            "".to_string(),
            "a\nb".to_string(),
            ".one".to_string(),
            ".two".to_string(),
            ".three".to_string(),
            ".four".to_string(),
        ];
        let build =
            build_demonstration(&events_with(vec![event]), "https://example.com/app", "now");
        let action = &build.demonstration.actions[1];
        assert_eq!(action.selector.as_deref(), Some("#go"));
        assert_eq!(action.fallbacks, vec![".one", ".two", ".three"]);
    }

    fn events_with(events: Vec<RawDemoEvent>) -> Vec<RawDemoEvent> {
        events
    }

    #[test]
    fn only_enter_escape_and_tab_are_recorded_as_presses() {
        let mut enter = raw("press", 1, "https://example.com/app");
        enter.key = Some("Enter".to_string());
        let mut letter = raw("press", 2, "https://example.com/app");
        letter.id = Some("letter".to_string());
        letter.key = Some("a".to_string());
        let build = build_demonstration(
            &[enter, letter],
            "https://example.com/app",
            "2026-11-26T10:00:00Z",
        );
        let presses = build
            .demonstration
            .actions
            .iter()
            .filter(|action| action.kind == "press")
            .collect::<Vec<_>>();
        assert_eq!(presses.len(), 1);
        assert_eq!(presses[0].key.as_deref(), Some("Enter"));
        assert!(build
            .warnings
            .iter()
            .any(|warning| warning.contains("key press")));
    }

    #[test]
    fn checkbox_state_is_kept_on_the_click() {
        let mut event = raw("click", 1, "https://example.com/app");
        event.selector = Some("#remember".to_string());
        event.checked = Some(true);
        let build =
            build_demonstration(&[event], "https://example.com/app", "2026-11-26T10:00:00Z");
        assert_eq!(build.demonstration.actions[1].checked, Some(true));
    }

    #[test]
    fn the_proposal_is_deterministic_and_never_claims_verification() {
        let action = DemoAction {
            kind: "click".to_string(),
            url: Some("https://example.com/app".to_string()),
            selector: Some("#go".to_string()),
            fallbacks: vec![".primary".to_string()],
            label: Some("Search".to_string()),
            text: None,
            key: None,
            sensitive: false,
            checked: None,
        };
        let draft = propose_skill_from_demonstration(&demo(vec![action]), "you").expect("proposal");
        assert_eq!(draft.id, "demo-example-com-20261126t100000");
        assert_eq!(draft.tools, ["browser_dom"]);
        assert!(draft.demonstration.is_some());
        assert!(draft.content.contains("## Replay"));
        assert!(draft
            .content
            .contains("Demonstrated on 2026-11-26T10:00:00Z"));
        assert!(!draft.content.contains("tests passed"));
        assert!(!draft
            .content
            .to_lowercase()
            .contains("verified by replaying"));
        let stored = draft.demonstration.clone().expect("demo");
        let again = propose_skill_from_demonstration(&stored, "you").expect("second proposal");
        assert_eq!(draft.id, again.id);
        assert_eq!(draft.content, again.content);
    }

    #[test]
    fn a_demo_without_actions_is_refused() {
        let error = propose_skill_from_demonstration(&demo(Vec::new()), "you")
            .expect_err("empty demonstration");
        assert!(error.contains("no recordable"), "{error}");
    }

    #[test]
    fn the_document_has_valid_frontmatter_and_marks_sensitive_steps() {
        let fill = DemoAction {
            kind: "fill".to_string(),
            url: Some("https://example.com/login".to_string()),
            selector: Some("#otp".to_string()),
            fallbacks: Vec::new(),
            label: Some("One-time code".to_string()),
            text: None,
            key: None,
            sensitive: true,
            checked: None,
        };
        let draft = propose_skill_from_demonstration(&demo(vec![fill]), "you").expect("proposal");
        assert!(draft.content.starts_with("---\n"));
        let rest = draft.content.strip_prefix("---\n").unwrap();
        let end = rest.find("\n---").expect("frontmatter closes");
        let header: serde_yaml::Value =
            serde_yaml::from_str(&rest[..end]).expect("valid frontmatter YAML");
        assert_eq!(header["name"].as_str(), Some(draft.name.as_str()));
        assert!(draft.content.contains("needs your input"));
        assert!(draft
            .warnings
            .iter()
            .any(|warning| warning.contains("sensitive")));
    }

    #[test]
    fn replay_scripts_embed_escaped_actions_and_return_the_helper_call() {
        let action = DemoAction {
            kind: "click".to_string(),
            url: Some("https://example.com/app".to_string()),
            selector: Some("a[href=\"x\"]\n'); alert(1); //".to_string()),
            fallbacks: Vec::new(),
            label: Some("A \"quoted\" label".to_string()),
            text: None,
            key: None,
            sensitive: false,
            checked: None,
        };
        let script = build_replay_script(&action).expect("script");
        assert!(script.contains("__ravenbotReplay.run("));
        assert!(script.contains("a[href=\\\"x\\\"]"));
        assert!(!script.contains("alert(1); //\n"));
        assert!(DEMO_REPLAY_JS.contains("__ravenbotReplay"));
    }

    #[test]
    fn recorder_scripts_are_available_and_include_the_asset() {
        assert!(DEMO_RECORDER_JS.contains("__ravenbotDemo"));
        assert!(build_recorder_start_script("https://example.com", 1, true).contains("fresh: true"));
        assert!(build_recorder_resume_script("https://example.com", 1).contains("fresh: false"));
        assert!(build_recorder_status_script(7).contains("status(7)"));
        assert!(build_recorder_stop_script(true).contains("clear: true"));
    }

    #[test]
    fn a_replay_step_is_parsed_without_trusting_its_shape() {
        let step = parse_replay_step(&json!({
            "ok": true,
            "matched_by": "fallback",
            "selector": ".primary",
            "page_url": "https://example.com/app",
            "attempted": ["#go", ".primary"]
        }));
        assert!(step.ok);
        assert_eq!(step.matched_by.as_deref(), Some("fallback"));
        assert_eq!(step.attempted.len(), 2);

        let broken = parse_replay_step(&json!({"ok": "yes"}));
        assert!(!broken.ok);
        assert!(broken.error.unwrap().contains("usable"));
    }

    #[test]
    fn repair_hints_name_the_selector_the_label_and_the_page() {
        let action = DemoAction {
            kind: "click".to_string(),
            url: Some("https://example.com/app".to_string()),
            selector: Some("#go".to_string()),
            fallbacks: vec![".primary".to_string()],
            label: Some("Search".to_string()),
            text: None,
            key: None,
            sensitive: false,
            checked: None,
        };
        assert_eq!(
            attempted_selectors(&action),
            ["#go", ".primary", "label:Search"]
        );
        let hint = replay_repair_hint(
            &action,
            "https://example.com/app",
            &attempted_selectors(&action),
            "No element matched",
        );
        assert!(hint.contains("#go"));
        assert!(hint.contains("Search"));
        assert!(hint.contains("did not change the skill"));
        let sensitive = sensitive_repair_hint(&action, "https://example.com/app");
        assert!(sensitive.contains("deliberately not recorded"));
        assert!(sensitive.contains("Search"));
        let early = replay_repair_hint(
            &action,
            "https://example.com/app",
            &[],
            "The dock was closed",
        );
        assert!(early.contains("before touching"));
        assert!(early.contains("did not change the skill"));
    }

    #[test]
    fn sensitive_labels_do_not_carry_the_secret() {
        let mut event = raw("fill", 1, "https://example.com/login");
        event.selector = Some("#password".to_string());
        event.label = Some("Password".to_string());
        event.text = Some("sk-live-abcdefghijklmnop".to_string());
        event.sensitive = true;
        let build = build_demonstration(
            &[event],
            "https://example.com/login",
            "2026-11-26T10:00:00Z",
        );
        let serialized = serde_json::to_string(&build.demonstration).unwrap();
        assert!(!serialized.contains("sk-live-abcdefghijklmnop"));
    }
}

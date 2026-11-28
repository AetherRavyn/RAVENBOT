//! Durable agent tasks and event waits.
//!
//! A task is background work one agent starts for another (or for itself). The
//! host's task loop claims queued tasks under dependency and resource rules,
//! runs them on a real thread, and fires waits when their state changes. A wait
//! is one blocked run's claim on an event: it fires once, is consumed once, and
//! expires rather than hanging.
//!
//! The types and validation here are deliberately narrow. Event filters are
//! exact-match fields, not a query language; task resources are explicit lock
//! keys, not filesystem grants; payloads are bounded so a task result cannot
//! become an unbounded message.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use uuid::Uuid;

/// Longest task title.
pub const MAX_TASK_TITLE: usize = 120;
/// Longest instruction.
pub const MAX_TASK_INSTRUCTION: usize = 8_000;
/// Longest expected-result contract.
pub const MAX_EXPECTED_RESULT: usize = 2_000;
/// Most resources one task may hold.
pub const MAX_TASK_RESOURCES: usize = 16;
/// Longest one resource key.
pub const MAX_RESOURCE_LEN: usize = 300;
/// Most dependencies one task may declare.
pub const MAX_DEPENDENCIES: usize = 16;
/// Most keys in an event filter.
pub const MAX_FILTER_KEYS: usize = 8;
/// Largest event payload handed to a waiting run.
pub const MAX_EVENT_PAYLOAD_BYTES: usize = 16 * 1024;
/// Largest string field inside an event payload.
pub const MAX_EVENT_FIELD_CHARS: usize = 4_000;
/// Default wait duration.
pub const DEFAULT_WAIT_MS: u64 = 60_000;
/// Longest a run may block on one wait.
pub const MAX_WAIT_MS: u64 = 600_000;

/// Life of one agent task.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentTaskState {
    /// Waiting for a claim, dependencies or a free resource.
    Queued,
    /// Claimed by the host loop but the run is not recorded yet.
    Starting,
    /// The task's run is executing.
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl AgentTaskState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn from_db(value: &str) -> Self {
        match value {
            "starting" => Self::Starting,
            "running" => Self::Running,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            _ => Self::Queued,
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

/// Life of one event wait.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentWaitState {
    Waiting,
    /// An event matched; the blocked tool has not consumed it yet.
    Fired,
    /// The blocked tool read the payload. A fired wait is never delivered twice.
    Consumed,
    Expired,
    Cancelled,
}

impl AgentWaitState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Fired => "fired",
            Self::Consumed => "consumed",
            Self::Expired => "expired",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn from_db(value: &str) -> Self {
        match value {
            "fired" => Self::Fired,
            "consumed" => Self::Consumed,
            "expired" => Self::Expired,
            "cancelled" => Self::Cancelled,
            _ => Self::Waiting,
        }
    }

    pub fn is_active(self) -> bool {
        matches!(self, Self::Waiting | Self::Fired)
    }
}

/// One durable background task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTask {
    pub id: Uuid,
    pub owner_bot_id: Uuid,
    pub target_bot_id: Uuid,
    pub owner_thread_id: Option<Uuid>,
    pub owner_run_id: Option<Uuid>,
    /// The account/chat actor that started the task, copied onto the task run
    /// so its tools enforce the same role boundary as the caller.
    pub actor_id: Option<String>,
    pub actor_name: Option<String>,
    pub actor_role: Option<String>,
    pub actor_workspace_id: Option<String>,
    pub thread_id: Option<Uuid>,
    pub run_id: Option<Uuid>,
    pub office_id: Option<Uuid>,
    pub title: String,
    pub instruction: String,
    pub expected_result: String,
    pub state: AgentTaskState,
    /// Lock keys such as `browser` or `workspace:/path`.
    pub resources: Vec<String>,
    pub depends_on: Vec<Uuid>,
    /// The workspace the task's thread resolves to, when one was known.
    pub workspace: Option<String>,
    /// Handoff depth, matching the delegation chain cap.
    pub depth: u32,
    pub auto_allow: bool,
    /// `allow_all` or `review`; `None` inherits the target's mode.
    pub approval_policy: Option<String>,
    pub result: Option<String>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

/// The prompt a task's target agent receives, including the task id so the
/// owner can wait for it and the target can refer to it.
#[allow(clippy::too_many_arguments)]
pub fn task_prompt(
    id: Uuid,
    owner_name: &str,
    title: &str,
    instruction: &str,
    expected_result: &str,
    resources: &[String],
) -> String {
    let mut text = format!(
        "[Background task from {owner_name}]\n\
         Task id: {id}\n\
         Title: {title}\n\n\
         Instruction:\n{instruction}\n"
    );
    if !expected_result.trim().is_empty() {
        text.push_str("\nExpected result:\n");
        text.push_str(expected_result.trim());
        text.push('\n');
    }
    if !resources.is_empty() {
        text.push_str("\nScheduled resources: ");
        text.push_str(&resources.join(", "));
        text.push('\n');
    }
    text.push_str(
        "\nThis is a background task. Do the work and finish with a concrete result. \
         If you are blocked on something only the owner can decide, say exactly what \
         you need rather than guessing.",
    );
    text
}

impl AgentTask {
    /// The prompt the target agent receives on its task thread.
    pub fn prompt(&self, owner_name: &str) -> String {
        task_prompt(
            self.id,
            owner_name,
            &self.title,
            &self.instruction,
            &self.expected_result,
            &self.resources,
        )
    }

    /// A bounded event payload for a state or message transition.
    pub fn event_payload(&self, event: &str, message: Option<&str>) -> Value {
        let mut value = json!({
            "event": event,
            "task_id": self.id,
            "owner_bot_id": self.owner_bot_id,
            "target_bot_id": self.target_bot_id,
            "title": self.title,
            "state": self.state.as_str(),
            "result": self.result,
            "error": self.error,
            "updated_at": self.updated_at,
        });
        if let Some(message) = message {
            value["message"] = json!(message);
        }
        bounded_event_payload(value)
    }
}

/// One blocked run's claim on an event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentWait {
    pub id: Uuid,
    pub bot_id: Uuid,
    pub run_id: Uuid,
    pub thread_id: Uuid,
    /// `task` or `timer`.
    pub event_kind: String,
    /// Exact-match fields for a task event. Empty matches any task event for
    /// the owning bot.
    pub filter: Value,
    pub deadline: DateTime<Utc>,
    pub state: AgentWaitState,
    pub payload: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub fired_at: Option<DateTime<Utc>>,
    pub consumed_at: Option<DateTime<Utc>>,
    pub closed_at: Option<DateTime<Utc>>,
}

/// Validate a task title.
pub fn validate_task_title(raw: &str) -> Result<String, String> {
    // A title is a one-line label, even when it is derived from an instruction
    // that starts with a paragraph or a code block.
    let title = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if title.is_empty() {
        return Err("A task needs a title.".to_string());
    }
    if title.chars().count() > MAX_TASK_TITLE {
        return Err(format!(
            "The task title is longer than {MAX_TASK_TITLE} characters."
        ));
    }
    Ok(title)
}

/// Validate a task instruction.
pub fn validate_task_instruction(raw: &str) -> Result<String, String> {
    let instruction = raw.trim();
    if instruction.is_empty() {
        return Err("A task needs an instruction.".to_string());
    }
    if instruction.chars().count() > MAX_TASK_INSTRUCTION {
        return Err(format!(
            "The task instruction is longer than {MAX_TASK_INSTRUCTION} characters."
        ));
    }
    Ok(instruction.to_string())
}

/// Validate the expected-result contract.
pub fn validate_expected_result(raw: &str) -> Result<String, String> {
    let expected = raw.trim();
    if expected.chars().count() > MAX_EXPECTED_RESULT {
        return Err(format!(
            "The expected result is longer than {MAX_EXPECTED_RESULT} characters."
        ));
    }
    Ok(expected.to_string())
}

/// Normalize and validate task resource keys.
pub fn normalize_resources(values: &[String]) -> Result<Vec<String>, String> {
    if values.len() > MAX_TASK_RESOURCES {
        return Err(format!(
            "A task may declare at most {MAX_TASK_RESOURCES} resources."
        ));
    }
    let mut out = Vec::new();
    for raw in values {
        let resource = normalize_resource(raw)?;
        if !out.contains(&resource) {
            out.push(resource);
        }
    }
    out.sort();
    Ok(out)
}

/// Normalize one resource key.
///
/// `browser` is the one shared desktop resource. `workspace:<path>` is a lock
/// key: two tasks touching the same path serialize, but the path grants no
/// access by itself. The task's real working directory still comes from its
/// thread and office.
pub fn normalize_resource(raw: &str) -> Result<String, String> {
    let raw = raw.trim();
    if raw.eq_ignore_ascii_case("browser") {
        return Ok("browser".to_string());
    }
    let Some(path) = raw.strip_prefix("workspace:") else {
        return Err(
            "Unknown task resource. Use `browser` or `workspace:<absolute path>`.".to_string(),
        );
    };
    let path = path.trim();
    if path.is_empty() {
        return Err("A workspace resource needs a path.".to_string());
    }
    if path.chars().any(char::is_control) {
        return Err("A workspace resource contains a control character.".to_string());
    }
    let expanded = crate::paths::expand_home(path);
    if !expanded.is_absolute() {
        return Err("A workspace resource must be an absolute path or start with ~/.".to_string());
    }
    let mut clean = std::path::PathBuf::new();
    for component in expanded.components() {
        use std::path::Component;
        match component {
            Component::Prefix(prefix) => clean.push(prefix.as_os_str()),
            Component::RootDir => clean.push(std::path::MAIN_SEPARATOR.to_string()),
            Component::CurDir => {}
            Component::ParentDir => {
                clean.pop();
            }
            Component::Normal(part) => clean.push(part),
        }
    }
    let text = clean.to_string_lossy().to_string();
    let text = if text.len() > 1 {
        text.trim_end_matches(std::path::MAIN_SEPARATOR).to_string()
    } else {
        text
    };
    if text.is_empty() || text.len() > MAX_RESOURCE_LEN {
        return Err(format!(
            "A workspace resource must be 1-{MAX_RESOURCE_LEN} characters."
        ));
    }
    Ok(format!("workspace:{text}"))
}

/// Validate the event kind supported by this slice.
pub fn validate_event_kind(raw: &str) -> Result<String, String> {
    let kind = raw.trim().to_ascii_lowercase();
    match kind.as_str() {
        "task" | "timer" => Ok(kind),
        _ => Err(
            "Unsupported event kind. Use `task` for task transitions/messages or `timer` for a deadline."
                .to_string(),
        ),
    }
}

/// Validate and normalise an exact-match task filter.
pub fn validate_event_filter(raw: &Value) -> Result<Value, String> {
    let Some(map) = raw.as_object() else {
        return Err("A task filter must be a JSON object.".to_string());
    };
    if map.len() > MAX_FILTER_KEYS {
        return Err(format!(
            "A task filter has at most {MAX_FILTER_KEYS} fields."
        ));
    }
    let mut out = Map::new();
    for (key, value) in map {
        match key.as_str() {
            "task_id" | "target_bot_id" => {
                let text = value
                    .as_str()
                    .ok_or_else(|| format!("`{key}` must be a UUID string."))?;
                let id = Uuid::parse_str(text.trim())
                    .map_err(|_| format!("`{key}` is not a valid UUID."))?;
                out.insert(key.clone(), json!(id.to_string()));
            }
            "state" => {
                let state = value
                    .as_str()
                    .ok_or_else(|| "`state` must be a string.".to_string())?;
                if !matches!(
                    state,
                    "queued" | "starting" | "running" | "completed" | "failed" | "cancelled"
                ) {
                    return Err(format!("Unknown task state '{state}'."));
                }
                out.insert("state".to_string(), json!(state));
            }
            other => {
                return Err(format!(
                    "Unknown task filter field '{other}'. Use task_id, state or target_bot_id."
                ))
            }
        }
    }
    Ok(Value::Object(out))
}

/// Whether an event payload satisfies an exact-match filter.
pub fn task_event_matches(filter: &Value, event: &Value) -> bool {
    let Some(filter) = filter.as_object() else {
        return false;
    };
    filter
        .iter()
        .all(|(key, wanted)| event.get(key) == Some(wanted))
}

/// Bound event strings and total size before a wait receives them.
pub fn bounded_event_payload(mut value: Value) -> Value {
    for key in ["title", "result", "error", "message", "instruction"] {
        let Some(text) = value.get(key).and_then(Value::as_str).map(str::to_string) else {
            continue;
        };
        if text.chars().count() > MAX_EVENT_FIELD_CHARS {
            let mut clipped: String = text.chars().take(MAX_EVENT_FIELD_CHARS).collect();
            clipped.push_str("… [truncated]");
            value[key] = json!(clipped);
        }
    }
    if serde_json::to_string(&value)
        .map(|text| text.len())
        .unwrap_or(usize::MAX)
        > MAX_EVENT_PAYLOAD_BYTES
    {
        return json!({
            "event": value.get("event").cloned().unwrap_or(Value::Null),
            "task_id": value.get("task_id").cloned().unwrap_or(Value::Null),
            "state": value.get("state").cloned().unwrap_or(Value::Null),
            "truncated": true,
        });
    }
    value
}

/// A timer event payload.
pub fn timer_event(deadline: DateTime<Utc>) -> Value {
    bounded_event_payload(json!({
        "event": "timer",
        "deadline": deadline.to_rfc3339(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_states_round_trip_and_terminal_states_are_closed() {
        for state in [
            AgentTaskState::Queued,
            AgentTaskState::Starting,
            AgentTaskState::Running,
            AgentTaskState::Completed,
            AgentTaskState::Failed,
            AgentTaskState::Cancelled,
        ] {
            assert_eq!(AgentTaskState::from_db(state.as_str()), state);
        }
        assert!(!AgentTaskState::Running.is_terminal());
        assert!(AgentTaskState::Completed.is_terminal());
        assert!(AgentTaskState::Failed.is_terminal());
        assert!(AgentTaskState::Cancelled.is_terminal());
    }

    #[test]
    fn a_title_is_a_single_line_even_when_it_starts_as_a_paragraph() {
        assert_eq!(
            validate_task_title("  Plan\n\nthe\t release ").unwrap(),
            "Plan the release"
        );
        assert!(validate_task_title("   ").is_err());
    }

    #[test]
    fn resources_are_normalized_and_unknown_prefixes_are_refused() {
        let resources = normalize_resources(&[
            "browser".to_string(),
            "workspace:~/work/../work/app".to_string(),
            "workspace:~/work/app".to_string(),
        ])
        .expect("resources");
        assert_eq!(resources.len(), 2);
        assert!(resources.contains(&"browser".to_string()));
        let expected = format!(
            "workspace:{}",
            crate::paths::expand_home("~/work/app").display()
        );
        assert!(resources.contains(&expected), "{resources:?}");
        assert!(normalize_resource("gpu").unwrap_err().contains("Unknown"));
        assert!(normalize_resource("workspace:relative").is_err());
        assert!(normalize_resource("workspace:").is_err());
    }

    #[test]
    fn filters_are_exact_and_bounded() {
        let task = Uuid::new_v4();
        let filter = validate_event_filter(&json!({
            "task_id": task.to_string(),
            "state": "completed"
        }))
        .expect("filter");
        assert!(task_event_matches(
            &filter,
            &json!({"task_id": task.to_string(), "state": "completed", "event": "state"})
        ));
        assert!(!task_event_matches(
            &filter,
            &json!({"task_id": task.to_string(), "state": "failed"})
        ));
        assert!(validate_event_filter(&json!({"unknown": "x"})).is_err());
        assert!(validate_event_filter(&json!({"task_id": "nope"})).is_err());
        assert!(validate_event_filter(&json!({"state": "sleeping"})).is_err());
    }

    #[test]
    fn a_large_result_is_clipped_before_it_reaches_a_wait() {
        let payload = bounded_event_payload(json!({
            "event": "state",
            "task_id": Uuid::new_v4().to_string(),
            "state": "completed",
            "result": "x".repeat(MAX_EVENT_FIELD_CHARS + 100),
        }));
        let text = payload["result"].as_str().unwrap();
        assert!(text.ends_with("[truncated]"));
        assert!(text.chars().count() <= MAX_EVENT_FIELD_CHARS + 14);
        assert!(serde_json::to_string(&payload).unwrap().len() < MAX_EVENT_PAYLOAD_BYTES);
    }

    #[test]
    fn a_task_prompt_carries_the_contract_but_not_hidden_state() {
        let task = AgentTask {
            id: Uuid::new_v4(),
            owner_bot_id: Uuid::new_v4(),
            target_bot_id: Uuid::new_v4(),
            owner_thread_id: None,
            owner_run_id: None,
            actor_id: None,
            actor_name: None,
            actor_role: None,
            actor_workspace_id: None,
            thread_id: None,
            run_id: None,
            office_id: None,
            title: "Review the migration".to_string(),
            instruction: "Check the rollback path.".to_string(),
            expected_result: "A yes/no with evidence.".to_string(),
            state: AgentTaskState::Queued,
            resources: vec!["browser".to_string()],
            depends_on: Vec::new(),
            workspace: None,
            depth: 1,
            auto_allow: false,
            approval_policy: None,
            result: None,
            error: None,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            updated_at: Utc::now(),
        };
        let prompt = task.prompt("The Raven");
        assert!(prompt.contains("Background task from The Raven"));
        assert!(prompt.contains("Check the rollback path."));
        assert!(prompt.contains("A yes/no with evidence."));
        assert!(prompt.contains("browser"));
    }
}

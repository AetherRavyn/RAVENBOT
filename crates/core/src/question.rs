use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Lifecycle of one `ask_user` question.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QuestionStatus {
    /// Waiting for the user (run is parked)
    Pending,
    /// User answered — the answer was fed back to the model
    Answered,
    /// The run finished or timed out before an answer
    Expired,
}

/// The shape of an interactive request.
///
/// `Question` is the original free-text/choice card. The other kinds are a
/// deliberately small, validated protocol: an agent can ask for a decision, a
/// selection, or a few fields without gaining the ability to render arbitrary
/// HTML or to invent controls the host does not understand.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum QuestionKind {
    /// Free text with optional suggested answers.
    #[default]
    Question,
    /// A decision with details, actions, and an optional hold to confirm.
    Confirm,
    /// A single- or multi-select list.
    Choice,
    /// A small set of labelled fields.
    Form,
    /// A request for a local-vault item. The answer carries only the chosen
    /// item and the result of a local browser fill, never the secret.
    Vault,
}

impl QuestionKind {
    /// The value stored in SQLite and sent over IPC.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Question => "question",
            Self::Confirm => "confirm",
            Self::Choice => "choice",
            Self::Form => "form",
            Self::Vault => "vault",
        }
    }

    /// Parse a value from the database, defaulting to the original card.
    pub fn from_db(value: &str) -> Self {
        match value {
            "confirm" => Self::Confirm,
            "choice" => Self::Choice,
            "form" => Self::Form,
            "vault" => Self::Vault,
            _ => Self::Question,
        }
    }
}

/// One selectable item in a `choice` block.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlockOption {
    pub id: String,
    pub label: String,
    /// A short note on the trailing edge, such as a count.
    #[serde(default)]
    pub meta: Option<String>,
    /// Selected when the block opens. At most one unless the block is multiple.
    #[serde(default)]
    pub selected: bool,
}

/// A field in a `form` block or a read-only line in a `confirm` block.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlockField {
    pub id: String,
    pub label: String,
    /// `text`, `textarea`, or `select`.
    #[serde(default = "default_field_kind")]
    pub kind: String,
    #[serde(default)]
    pub required: bool,
    /// The choices of a `select` field.
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub placeholder: Option<String>,
    /// A prefilled value. A confirm field with no `kind` uses this as a
    /// read-only detail line.
    #[serde(default)]
    pub value: Option<String>,
}

fn default_field_kind() -> String {
    "text".to_string()
}

/// A button in a `confirm` block.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlockAction {
    pub id: String,
    pub label: String,
    /// `primary`, `secondary`, or `danger`.
    #[serde(default)]
    pub style: Option<String>,
    /// Draws the action as dangerous and requires the hold control.
    #[serde(default)]
    pub danger: bool,
    /// Ask the user again before sending this action.
    #[serde(default)]
    pub confirm: bool,
    /// The hold duration for a dangerous action, in milliseconds.
    #[serde(default)]
    pub hold_ms: Option<u32>,
}

/// The payload of a non-question interaction block.
///
/// These are structures rather than arbitrary JSON so the host can validate
/// every field before it is persisted or rendered. The UI never interprets a
/// field as HTML or Markdown; `preview` is the only prose field and is rendered
/// through the app's safe Markdown renderer.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct InteractionBlock {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// Draws the whole card as dangerous.
    #[serde(default)]
    pub danger: bool,
    /// A choice block may select more than one option.
    #[serde(default)]
    pub multiple: bool,
    /// A choice block may also accept typed text.
    #[serde(default)]
    pub allow_text: bool,
    #[serde(default)]
    pub submit_label: Option<String>,
    /// A safe-Markdown preview of what will happen.
    #[serde(default)]
    pub preview: Option<String>,
    /// The site a `vault` request is for, used to reject a fill on the wrong
    /// origin.
    #[serde(default)]
    pub site: Option<String>,
    /// The agent's one-line explanation of why it needs the item.
    #[serde(default)]
    pub purpose: Option<String>,
    /// The vault item kind requested. Only `login` is supported for browser
    /// fill today.
    #[serde(default)]
    pub item_kind: Option<String>,
    #[serde(default)]
    pub options: Vec<BlockOption>,
    #[serde(default)]
    pub fields: Vec<BlockField>,
    #[serde(default)]
    pub actions: Vec<BlockAction>,
}

const MAX_TITLE: usize = 200;
const MAX_LABEL: usize = 120;
const MAX_DESCRIPTION: usize = 2_000;
const MAX_VALUE: usize = 8_000;
const MAX_OPTIONS: usize = 20;
const MAX_FIELDS: usize = 20;
const MAX_ACTIONS: usize = 4;
const MIN_HOLD_MS: u32 = 400;
const MAX_HOLD_MS: u32 = 3_000;

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | ':' | '-'))
}

fn check_text(value: Option<&str>, max: usize, label: &str) -> Result<(), String> {
    if let Some(value) = value {
        if value.chars().count() > max {
            return Err(format!("{label} is longer than {max} characters"));
        }
    }
    Ok(())
}

impl InteractionBlock {
    /// Validate a block before it is persisted.
    ///
    /// The runtime calls this once, and the UI trusts the stored shape. A bad
    /// block is a tool error the model reads, not a broken card the user sees.
    pub fn validate(&self, kind: QuestionKind) -> Result<(), String> {
        check_text(self.title.as_deref(), MAX_TITLE, "title")?;
        check_text(self.description.as_deref(), MAX_DESCRIPTION, "description")?;
        check_text(self.preview.as_deref(), MAX_DESCRIPTION, "preview")?;
        check_text(self.submit_label.as_deref(), MAX_LABEL, "submit label")?;

        match kind {
            QuestionKind::Question => Ok(()),
            QuestionKind::Confirm => {
                if self.actions.is_empty() {
                    return Err("a confirm block needs at least one action".into());
                }
                if self.actions.len() > MAX_ACTIONS {
                    return Err(format!("a confirm block has at most {MAX_ACTIONS} actions"));
                }
                if self.fields.len() > 8 {
                    return Err("a confirm block has at most 8 detail fields".into());
                }
                let mut ids = std::collections::HashSet::new();
                for action in &self.actions {
                    if !valid_id(&action.id) {
                        return Err(format!("invalid action id '{}'", action.id));
                    }
                    if !ids.insert(action.id.as_str()) {
                        return Err(format!("duplicate action id '{}'", action.id));
                    }
                    check_text(Some(&action.label), MAX_LABEL, "action label")?;
                    if let Some(style) = &action.style {
                        if !matches!(style.as_str(), "primary" | "secondary" | "danger") {
                            return Err(format!("unknown action style '{style}'"));
                        }
                    }
                    if let Some(hold) = action.hold_ms {
                        if !(MIN_HOLD_MS..=MAX_HOLD_MS).contains(&hold) {
                            return Err(format!(
                                "hold_ms must be between {MIN_HOLD_MS} and {MAX_HOLD_MS}"
                            ));
                        }
                    }
                }
                self.validate_fields(false)
            }
            QuestionKind::Choice => {
                if self.options.is_empty() {
                    return Err("a choice block needs at least one option".into());
                }
                if self.options.len() > MAX_OPTIONS {
                    return Err(format!("a choice block has at most {MAX_OPTIONS} options"));
                }
                let mut ids = std::collections::HashSet::new();
                for option in &self.options {
                    if !valid_id(&option.id) {
                        return Err(format!("invalid option id '{}'", option.id));
                    }
                    if !ids.insert(option.id.as_str()) {
                        return Err(format!("duplicate option id '{}'", option.id));
                    }
                    check_text(Some(&option.label), MAX_LABEL, "option label")?;
                    check_text(option.meta.as_deref(), 60, "option note")?;
                }
                if !self.multiple
                    && self.options.iter().filter(|option| option.selected).count() > 1
                {
                    return Err("a single-select choice has more than one selected option".into());
                }
                Ok(())
            }
            QuestionKind::Form => {
                if self.fields.is_empty() {
                    return Err("a form needs at least one field".into());
                }
                if self.fields.len() > MAX_FIELDS {
                    return Err(format!("a form has at most {MAX_FIELDS} fields"));
                }
                self.validate_fields(true)
            }
            QuestionKind::Vault => {
                let site = self.site.as_deref().unwrap_or("").trim();
                if site.is_empty() || site.chars().count() > 300 {
                    return Err("a vault request needs a site of at most 300 characters".into());
                }
                check_text(self.purpose.as_deref(), 500, "purpose")?;
                let item_kind = self.item_kind.as_deref().unwrap_or("login");
                if item_kind != "login" {
                    return Err("only login vault items can be filled into the browser".into());
                }
                Ok(())
            }
        }
    }

    fn validate_fields(&self, form: bool) -> Result<(), String> {
        let mut ids = std::collections::HashSet::new();
        for field in &self.fields {
            if !valid_id(&field.id) {
                return Err(format!("invalid field id '{}'", field.id));
            }
            if !ids.insert(field.id.as_str()) {
                return Err(format!("duplicate field id '{}'", field.id));
            }
            check_text(Some(&field.label), MAX_LABEL, "field label")?;
            check_text(field.placeholder.as_deref(), MAX_LABEL, "placeholder")?;
            check_text(field.value.as_deref(), MAX_VALUE, "field value")?;
            if form {
                if !matches!(field.kind.as_str(), "text" | "textarea" | "select") {
                    return Err(format!("unknown field kind '{}'", field.kind));
                }
                if field.kind == "select" {
                    if field.options.is_empty() {
                        return Err(format!("select field '{}' has no options", field.id));
                    }
                    if field.options.len() > MAX_OPTIONS {
                        return Err(format!(
                            "select field '{}' has more than {MAX_OPTIONS} options",
                            field.id
                        ));
                    }
                    for option in &field.options {
                        check_text(Some(option), MAX_LABEL, "select option")?;
                    }
                    if let Some(value) = &field.value {
                        if !value.is_empty() && !field.options.contains(value) {
                            return Err(format!(
                                "select field '{}' has a value outside its options",
                                field.id
                            ));
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

/// A question the agent asked the user mid-run (human-in-the-loop).
/// Persisted and streamed so the chat can render an answer card inline and
/// the run resumes with the chosen answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionRequest {
    /// Unique identifier
    pub id: Uuid,
    /// Bot that asked
    pub bot_id: Uuid,
    /// Thread the question belongs to (card renders here)
    pub thread_id: Uuid,
    /// Run parked waiting on the answer
    pub run_id: Uuid,
    /// Short card header ("Which database?")
    pub header: String,
    /// Full question text
    pub question: String,
    /// Optional predefined choices rendered as buttons
    #[serde(default)]
    pub options: Vec<String>,
    /// Whether a free-text answer is also allowed
    #[serde(default)]
    pub allow_custom: bool,
    /// The shape of the request. Older rows are `Question`.
    #[serde(default)]
    pub kind: QuestionKind,
    /// The validated payload of a `Confirm`, `Choice`, or `Form` block.
    #[serde(default)]
    pub payload: Option<InteractionBlock>,
    /// Current lifecycle state
    pub status: QuestionStatus,
    /// The user's answer (set when answered)
    pub answer: Option<String>,
    /// Created timestamp
    pub created_at: DateTime<Utc>,
    /// Answered timestamp
    pub answered_at: Option<DateTime<Utc>>,
}

impl QuestionRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn pending(
        bot_id: Uuid,
        thread_id: Uuid,
        run_id: Uuid,
        header: impl Into<String>,
        question: impl Into<String>,
        options: Vec<String>,
        allow_custom: bool,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            bot_id,
            thread_id,
            run_id,
            header: header.into(),
            question: question.into(),
            options,
            allow_custom,
            kind: QuestionKind::Question,
            payload: None,
            status: QuestionStatus::Pending,
            answer: None,
            created_at: Utc::now(),
            answered_at: None,
        }
    }

    /// Build a request around a validated structured block.
    #[allow(clippy::too_many_arguments)]
    pub fn pending_block(
        bot_id: Uuid,
        thread_id: Uuid,
        run_id: Uuid,
        header: impl Into<String>,
        question: impl Into<String>,
        kind: QuestionKind,
        payload: InteractionBlock,
    ) -> Result<Self, String> {
        if kind == QuestionKind::Question {
            return Err("a structured block cannot use the question kind".into());
        }
        payload.validate(kind)?;
        Ok(Self {
            id: Uuid::new_v4(),
            bot_id,
            thread_id,
            run_id,
            header: header.into(),
            question: question.into(),
            options: Vec::new(),
            allow_custom: payload.allow_text,
            kind,
            payload: Some(payload),
            status: QuestionStatus::Pending,
            answer: None,
            created_at: Utc::now(),
            answered_at: None,
        })
    }

    /// Parse the stored answer for a structured block.
    ///
    /// A structured answer is JSON. A client that answered with plain words
    /// (a fallback card) still gets a useful object rather than a parse error.
    pub fn structured_answer(&self) -> Option<serde_json::Value> {
        if self.kind == QuestionKind::Question {
            return None;
        }
        let answer = self.answer.as_deref()?.trim();
        if answer.is_empty() {
            return None;
        }
        serde_json::from_str(answer).ok().or_else(|| {
            Some(serde_json::json!({
                "action_id": "_text",
                "text": answer,
            }))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn option(id: &str) -> BlockOption {
        BlockOption {
            id: id.into(),
            label: id.into(),
            meta: None,
            selected: false,
        }
    }

    #[test]
    fn a_choice_needs_unique_options() {
        let block = InteractionBlock {
            title: Some("Pick one".into()),
            options: vec![option("a"), option("a")],
            ..Default::default()
        };
        assert!(block
            .validate(QuestionKind::Choice)
            .unwrap_err()
            .contains("duplicate"));
    }

    #[test]
    fn a_single_choice_cannot_prefill_two_selections() {
        let block = InteractionBlock {
            options: vec![
                BlockOption {
                    selected: true,
                    ..option("a")
                },
                BlockOption {
                    selected: true,
                    ..option("b")
                },
            ],
            ..Default::default()
        };
        assert!(block
            .validate(QuestionKind::Choice)
            .unwrap_err()
            .contains("more than one"));
    }

    #[test]
    fn a_form_rejects_an_unknown_field_kind_and_a_select_without_options() {
        let unknown = InteractionBlock {
            fields: vec![BlockField {
                id: "when".into(),
                label: "When".into(),
                kind: "date".into(),
                required: false,
                options: Vec::new(),
                placeholder: None,
                value: None,
            }],
            ..Default::default()
        };
        assert!(unknown
            .validate(QuestionKind::Form)
            .unwrap_err()
            .contains("unknown field kind"));

        let select = InteractionBlock {
            fields: vec![BlockField {
                id: "team".into(),
                label: "Team".into(),
                kind: "select".into(),
                required: true,
                options: Vec::new(),
                placeholder: None,
                value: None,
            }],
            ..Default::default()
        };
        assert!(select
            .validate(QuestionKind::Form)
            .unwrap_err()
            .contains("no options"));
    }

    #[test]
    fn a_dangerous_action_hold_is_bounded() {
        let block = InteractionBlock {
            actions: vec![BlockAction {
                id: "delete".into(),
                label: "Delete".into(),
                style: Some("danger".into()),
                danger: true,
                confirm: true,
                hold_ms: Some(50),
            }],
            ..Default::default()
        };
        assert!(block
            .validate(QuestionKind::Confirm)
            .unwrap_err()
            .contains("hold_ms"));
    }

    #[test]
    fn a_vault_request_needs_a_site_and_only_supports_logins() {
        let missing = InteractionBlock::default();
        assert!(missing
            .validate(QuestionKind::Vault)
            .unwrap_err()
            .contains("site"));
        let card = InteractionBlock {
            site: Some("https://github.com".into()),
            item_kind: Some("card".into()),
            ..Default::default()
        };
        assert!(card
            .validate(QuestionKind::Vault)
            .unwrap_err()
            .contains("login"));
        let login = InteractionBlock {
            site: Some("https://github.com".into()),
            item_kind: Some("login".into()),
            purpose: Some("Sign in to check the issue".into()),
            ..Default::default()
        };
        assert!(login.validate(QuestionKind::Vault).is_ok());
    }

    #[test]
    fn a_structured_answer_survives_a_fallback_string() {
        let mut request = QuestionRequest::pending_block(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            "Deploy",
            "Deploy now?",
            QuestionKind::Confirm,
            InteractionBlock {
                actions: vec![BlockAction {
                    id: "yes".into(),
                    label: "Yes".into(),
                    style: Some("primary".into()),
                    danger: false,
                    confirm: false,
                    hold_ms: None,
                }],
                ..Default::default()
            },
        )
        .unwrap();
        request.answer = Some(r#"{"action_id":"yes"}"#.into());
        assert_eq!(request.structured_answer().unwrap()["action_id"], "yes");
        request.answer = Some("yes".into());
        assert_eq!(request.structured_answer().unwrap()["action_id"], "_text");
    }
}

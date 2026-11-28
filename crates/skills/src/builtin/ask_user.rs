//! ask_user — human-in-the-loop question card.
//!
//! The runtime intercepts this tool by name and parks the run on a question
//! card (see `Runtime::ask_user`), feeding the answer back as the tool result.
//! This skill exists so the model sees `ask_user` in its tool list and gets a
//! sensible fallback if it is ever executed outside the runtime.

use crate::traits::{Skill, SkillContext, SkillError, SkillResult, SkillRisk};
use async_trait::async_trait;
use ravenbot_core::Permission;

pub struct AskUserSkill;

impl AskUserSkill {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Skill for AskUserSkill {
    fn id(&self) -> &str {
        "ask_user"
    }
    fn name(&self) -> &str {
        "Ask the user"
    }
    fn description(&self) -> &str {
        "Ask the user for input mid-task and wait for the answer. Use when you are \
         genuinely blocked on a decision only the user can make. `kind: question` \
         (the default) shows suggested answers and an optional text box. \
         `kind: confirm` shows a decision card with up to four actions and an \
         optional hold-to-confirm dangerous action. `kind: choice` shows a \
         single- or multi-select list. `kind: form` collects labelled text, \
         textarea, or select fields. `kind: vault` with `item_kind: login` \
         fills the current browser page from the local Vault; with \
         `item_kind: api_key` it opens a secure input, stores the key in the \
         Vault and returns only its name and owner-only file path. For a \
         structured kind, put the card in `payload` and still give a short \
         fallback `question`."
    }
    fn version(&self) -> &str {
        "1.0.0"
    }
    fn required_permissions(&self) -> Vec<Permission> {
        vec![]
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "question": { "type": "string", "description": "The question to ask the user" },
                "header": { "type": "string", "description": "Short card title (e.g. 'Which database?')" },
                "options": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Suggested answers rendered as buttons"
                },
                "allow_custom": {
                    "type": "boolean",
                    "description": "Whether a free-text answer is allowed (default true, question kind only)"
                },
                "kind": {
                    "type": "string",
                    "enum": ["question", "confirm", "choice", "form", "vault"],
                    "description": "Card shape. Default question."
                },
                "payload": {
                    "type": "object",
                    "description": "The block. confirm: {title, description?, danger?, preview?, fields?: [{id,label,value}], actions: [{id,label,style?,danger?,confirm?,hold_ms?}]}. choice: {title, description?, multiple?, allow_text?, options: [{id,label,meta?,selected?}], submit_label?}. form: {title, description?, fields: [{id,label,kind:text|textarea|select,required?,options?,placeholder?,value?}], submit_label?}. vault login: {site: string, purpose?, item_kind?: \"login\"}; the user fills the current browser page from the local vault and the tool result contains only the item name, origin and whether the fill succeeded, never the password. vault api_key: {item_kind: \"api_key\", name: string, title?, description?, site?, placeholder?, submit_label?}; the owner enters the key in a secure card, it is stored in the Vault, and the tool result contains only the name and owner-only file path, never the value."
                }
            },
            "required": ["question"]
        })
    }
    fn risk(&self) -> SkillRisk {
        SkillRisk::ReadOnly
    }

    async fn execute(
        &self,
        _ctx: &SkillContext,
        args: serde_json::Value,
    ) -> Result<SkillResult, SkillError> {
        let question = args
            .get("question")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing question".into()))?;
        // Reached only when executed outside the runtime's interception path.
        Ok(SkillResult::success(serde_json::json!({
            "answer": null,
            "unavailable": true,
            "note": format!("No interactive channel is available to answer: {}", question),
        })))
    }
}

impl Default for AskUserSkill {
    fn default() -> Self {
        Self::new()
    }
}

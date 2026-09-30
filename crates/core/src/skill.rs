use serde::{Deserialize, Serialize};
use crate::bot::Permission;

/// A skill that a bot can use
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    /// Unique identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Description of what the skill does
    pub description: String,
    /// Version
    pub version: String,
    /// Permissions required by this skill
    pub required_permissions: Vec<Permission>,
    /// JSON schema for tool call arguments
    pub input_schema: serde_json::Value,
    /// Whether this is a built-in skill or a plugin
    pub is_builtin: bool,
    /// Whether this skill is signed
    pub is_signed: bool,
}

impl Skill {
    /// Create a new skill
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            version: version.into(),
            required_permissions: Vec::new(),
            input_schema: serde_json::json!({}),
            is_builtin: true,
            is_signed: true,
        }
    }

    /// Set required permissions
    pub fn with_permissions(mut self, permissions: Vec<Permission>) -> Self {
        self.required_permissions = permissions;
        self
    }

    /// Set input schema
    pub fn with_input_schema(mut self, schema: serde_json::Value) -> Self {
        self.input_schema = schema;
        self
    }
}

/// Registry of available skills
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRegistry {
    /// All available skills
    pub skills: Vec<Skill>,
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new_builtin()
    }
}

impl SkillRegistry {
    /// Create a registry with built-in skills
    pub fn new_builtin() -> Self {
        Self {
            skills: vec![
                Skill::new(
                    "web_search",
                    "Web Search",
                    "Search the web for information",
                    "1.0.0",
                )
                .with_permissions(vec![Permission::Network {
                    domains: vec!["*".to_string()],
                }])
                .with_input_schema(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "The search query"
                        }
                    },
                    "required": ["query"]
                })),
                Skill::new(
                    "file_read",
                    "File Read",
                    "Read files from the filesystem",
                    "1.0.0",
                )
                .with_permissions(vec![Permission::FileSystem {
                    paths: vec!["/".to_string()],
                }])
                .with_input_schema(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the file"
                        }
                    },
                    "required": ["path"]
                })),
                Skill::new(
                    "file_write",
                    "File Write",
                    "Write files to the filesystem",
                    "1.0.0",
                )
                .with_permissions(vec![Permission::FileSystem {
                    paths: vec!["/".to_string()],
                }])
                .with_input_schema(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to write to"
                        },
                        "content": {
                            "type": "string",
                            "description": "Content to write"
                        }
                    },
                    "required": ["path", "content"]
                })),
                Skill::new(
                    "shell_exec",
                    "Shell Execute",
                    "Execute a shell command",
                    "1.0.0",
                )
                .with_permissions(vec![Permission::Shell])
                .with_input_schema(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The command to execute"
                        }
                    },
                    "required": ["command"]
                })),
                Skill::new(
                    "delegate",
                    "Delegate",
                    "Delegate a task to another bot",
                    "1.0.0",
                )
                .with_permissions(vec![Permission::Delegation])
                .with_input_schema(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "bot_id": {
                            "type": "string",
                            "description": "ID of the bot to delegate to"
                        },
                        "instruction": {
                            "type": "string",
                            "description": "The instruction to give the bot"
                        }
                    },
                    "required": ["bot_id", "instruction"]
                })),
            ],
        }
    }
}

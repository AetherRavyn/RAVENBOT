//! State management for the runtime

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Shared scratchpad for a task graph execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Blackboard {
    /// Shared context data
    pub data: HashMap<String, serde_json::Value>,
}

impl Blackboard {
    /// Create a new empty blackboard
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }

    /// Set a value on the blackboard
    pub fn set(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.data.insert(key.into(), value);
    }

    /// Get a value from the blackboard
    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.data.get(key)
    }

    /// Remove a value from the blackboard
    pub fn remove(&mut self, key: &str) -> Option<serde_json::Value> {
        self.data.remove(key)
    }
}

impl Default for Blackboard {
    fn default() -> Self {
        Self::new()
    }
}

/// A message in the conversation (for state tracking)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

/// A tool call request from the model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub name: String,
    pub arguments: serde_json::Value,
    pub id: String,
}

/// State of a run execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunStateData {
    /// Current step in the plan/act/observe/reflect cycle
    pub step: String,
    /// Messages in the current conversation
    pub messages: Vec<Message>,
    /// Pending tool calls
    pub pending_tool_calls: Vec<ToolCall>,
    /// Blackboard for shared state
    pub blackboard: Blackboard,
}

impl RunStateData {
    /// Create initial state
    pub fn new() -> Self {
        Self {
            step: "planning".to_string(),
            messages: Vec::new(),
            pending_tool_calls: Vec::new(),
            blackboard: Blackboard::new(),
        }
    }
}

impl Default for RunStateData {
    fn default() -> Self {
        Self::new()
    }
}

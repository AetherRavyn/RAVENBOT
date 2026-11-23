//! Completion contracts for long-running goals.
//!
//! A contract is the checkable half of an office mission: the outcome that
//! counts as done, the boundaries that must not be crossed, the turn/token/time
//! budget, and the commands that must pass before a model may claim completion.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    #[default]
    Active,
    Paused,
    Done,
    Blocked,
    Failed,
    Cancelled,
}

impl GoalStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Done => "done",
            Self::Blocked => "blocked",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "active" => Some(Self::Active),
            "paused" => Some(Self::Paused),
            "done" => Some(Self::Done),
            "blocked" => Some(Self::Blocked),
            "failed" => Some(Self::Failed),
            "cancelled" | "canceled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GoalVerdict {
    Done,
    #[default]
    Continue,
    Blocked,
    Wait,
}

impl GoalVerdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Continue => "continue",
            Self::Blocked => "blocked",
            Self::Wait => "wait",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "done" => Some(Self::Done),
            "continue" => Some(Self::Continue),
            "blocked" => Some(Self::Blocked),
            "wait" => Some(Self::Wait),
            _ => None,
        }
    }
}

/// One verification command that must pass before the goal can be done.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalGate {
    pub id: Uuid,
    pub command: String,
    /// Working directory. `None` means the office workspace root. A relative
    /// path is resolved against that root and must stay inside it.
    pub cwd: Option<String>,
    pub timeout_secs: u64,
    pub required: bool,
    pub position: u32,
}

impl GoalGate {
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            command: command.into(),
            cwd: None,
            timeout_secs: 600,
            required: true,
            position: 0,
        }
    }
}

/// The persisted completion contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalContract {
    pub objective: String,
    pub outcome: String,
    #[serde(default)]
    pub constraints: Vec<String>,
    #[serde(default)]
    pub boundaries: Vec<String>,
    pub stop_when: String,
    #[serde(default)]
    pub gates: Vec<GoalGate>,
    /// Maximum reviewed cycles for this goal. Zero means use the office default.
    pub max_turns: u32,
    /// Maximum tokens for this goal. Zero means no contract-level cap.
    pub max_tokens: u64,
    /// Maximum wall seconds for this goal. Zero means no contract-level cap.
    pub max_wall_seconds: u64,
}

impl Default for GoalContract {
    fn default() -> Self {
        Self {
            objective: String::new(),
            outcome: String::new(),
            constraints: Vec::new(),
            boundaries: Vec::new(),
            stop_when: String::new(),
            gates: Vec::new(),
            max_turns: 8,
            max_tokens: 0,
            max_wall_seconds: 0,
        }
    }
}

impl GoalContract {
    pub fn for_objective(objective: impl Into<String>) -> Self {
        let objective = objective.into();
        Self {
            objective,
            outcome: "The goal's deliverable exists and the verification passes.".to_string(),
            stop_when: "The lead review says done and every required gate passes.".to_string(),
            ..Default::default()
        }
    }

    /// Keep the contract internally consistent after a user edit.
    pub fn normalize(&mut self) {
        self.objective = self.objective.trim().to_string();
        self.outcome = self.outcome.trim().to_string();
        self.stop_when = self.stop_when.trim().to_string();
        self.constraints = self
            .constraints
            .iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect();
        self.boundaries = self
            .boundaries
            .iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect();
        self.gates = self
            .gates
            .iter()
            .filter_map(|gate| {
                let command = gate.command.trim();
                if command.is_empty() {
                    return None;
                }
                let mut gate = gate.clone();
                gate.command = command.to_string();
                gate.timeout_secs = gate.timeout_secs.clamp(1, 3_600);
                gate.cwd = gate
                    .cwd
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string);
                Some(gate)
            })
            .take(4)
            .enumerate()
            .map(|(position, mut gate)| {
                gate.position = position as u32;
                gate
            })
            .collect();
        self.max_turns = self.max_turns.clamp(1, 64);
    }
}

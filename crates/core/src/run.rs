use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use uuid::Uuid;

/// State of a task execution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RunState {
    /// Plan the next action
    Planning,
    /// Execute an action
    Acting,
    /// Observe the result
    Observing,
    /// Reflect on the outcome
    Reflecting,
    /// Waiting for user input
    WaitingOnUser,
    /// Paused by user
    Paused,
    /// Completed successfully
    Completed,
    /// Failed
    Failed,
    /// Cancelled by user
    Cancelled,
}

/// Outcome of a completed run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RunOutcome {
    /// Successful completion
    Success { result: String },
    /// Partial success
    PartialSuccess { result: String, errors: Vec<String> },
    /// Failure
    Failure { error: String },
    /// Cancelled
    Cancelled { reason: Option<String> },
}

/// A checkpoint for resuming a run after a crash
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunCheckpoint {
    /// Current state in the plan/act/observe/reflect cycle
    pub state: RunState,
    /// Serialized state machine snapshot (type-specific)
    pub state_data: serde_json::Value,
    /// Timestamp of this checkpoint
    pub timestamp: DateTime<Utc>,
}

/// A run of a bot executing a task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Run {
    /// Unique identifier
    pub id: Uuid,
    /// Bot that is running
    pub bot_id: Uuid,
    /// Thread this run belongs to
    pub thread_id: Uuid,
    /// Parent run (if this is a sub-run from delegation)
    pub parent_run_id: Option<Uuid>,
    /// Current state
    pub state: RunState,
    /// Checkpoint for crash recovery
    pub checkpoint: Option<RunCheckpoint>,
    /// Final outcome (if completed)
    pub outcome: Option<RunOutcome>,
    /// Total tokens consumed
    pub tokens_consumed: u64,
    /// Estimated cost in dollars
    pub cost_estimate: f64,
    /// Created timestamp
    pub created_at: DateTime<Utc>,
    /// Last updated timestamp
    pub updated_at: DateTime<Utc>,
    /// Completed timestamp
    pub completed_at: Option<DateTime<Utc>>,
}

impl Run {
    /// Create a new run
    pub fn new(bot_id: Uuid, thread_id: Uuid) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            bot_id,
            thread_id,
            parent_run_id: None,
            state: RunState::Planning,
            checkpoint: None,
            outcome: None,
            tokens_consumed: 0,
            cost_estimate: 0.0,
            created_at: now,
            updated_at: now,
            completed_at: None,
        }
    }

    /// Create a sub-run (from delegation)
    pub fn new_sub_run(bot_id: Uuid, thread_id: Uuid, parent_run_id: Uuid) -> Self {
        let mut run = Self::new(bot_id, thread_id);
        run.parent_run_id = Some(parent_run_id);
        run
    }

    /// Update the checkpoint
    pub fn checkpoint(&mut self, state_data: serde_json::Value) {
        self.checkpoint = Some(RunCheckpoint {
            state: self.state.clone(),
            state_data,
            timestamp: Utc::now(),
        });
        self.updated_at = Utc::now();
    }

    /// Mark as completed
    pub fn complete(&mut self, outcome: RunOutcome) {
        self.outcome = Some(outcome);
        self.state = RunState::Completed;
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    /// Add tokens and cost
    pub fn add_usage(&mut self, tokens: u64, cost: f64) {
        self.tokens_consumed += tokens;
        self.cost_estimate += cost;
        self.updated_at = Utc::now();
    }
}

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
    /// Estimated cost in dollars (model + tool; utility calls are recorded in
    /// their own ledger because they have no run).
    pub cost_estimate: f64,
    /// Portion spent on interactive model calls.
    pub model_cost: f64,
    /// Portion spent on paid tool calls.
    pub tool_cost: f64,
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
            model_cost: 0.0,
            tool_cost: 0.0,
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

    /// Mark as terminal. The state follows the outcome: a failed or cancelled
    /// run is not a completed one, and the usage screen counts those states.
    /// The checkpoint is dropped because it is a resume aid, not history.
    pub fn complete(&mut self, outcome: RunOutcome) {
        self.state = match outcome {
            RunOutcome::Success { .. } | RunOutcome::PartialSuccess { .. } => RunState::Completed,
            RunOutcome::Failure { .. } => RunState::Failed,
            RunOutcome::Cancelled { .. } => RunState::Cancelled,
        };
        self.outcome = Some(outcome);
        self.checkpoint = None;
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    /// Add model tokens and cost.
    ///
    /// Kept under the old name for callers that only ever recorded model
    /// calls. New code should say which bucket it is charging.
    pub fn add_usage(&mut self, tokens: u64, cost: f64) {
        self.add_model_usage(tokens, cost);
    }

    /// Add tokens and cost from an interactive model call.
    pub fn add_model_usage(&mut self, tokens: u64, cost: f64) {
        self.tokens_consumed += tokens;
        self.model_cost += cost;
        self.cost_estimate += cost;
        self.updated_at = Utc::now();
    }

    /// Add a paid tool charge. Tool calls do not consume model tokens.
    pub fn add_tool_usage(&mut self, cost: f64) {
        self.tool_cost += cost;
        self.cost_estimate += cost;
        self.updated_at = Utc::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_terminal_outcome_sets_the_matching_state_and_clears_the_checkpoint() {
        for (outcome, expected) in [
            (
                RunOutcome::Success {
                    result: "ok".into(),
                },
                RunState::Completed,
            ),
            (
                RunOutcome::PartialSuccess {
                    result: "part".into(),
                    errors: vec!["x".into()],
                },
                RunState::Completed,
            ),
            (
                RunOutcome::Failure {
                    error: "bad".into(),
                },
                RunState::Failed,
            ),
            (RunOutcome::Cancelled { reason: None }, RunState::Cancelled),
        ] {
            let mut run = Run::new(Uuid::new_v4(), Uuid::new_v4());
            run.checkpoint(serde_json::json!({"model_messages": []}));
            run.complete(outcome);
            assert_eq!(run.state, expected);
            assert!(
                run.checkpoint.is_none(),
                "checkpoint must not survive a terminal outcome"
            );
            assert!(run.completed_at.is_some());
        }
    }

    #[test]
    fn model_and_tool_spend_are_tracked_separately() {
        let mut run = Run::new(Uuid::new_v4(), Uuid::new_v4());
        run.add_model_usage(120, 0.40);
        run.add_tool_usage(0.05);
        run.add_model_usage(80, 0.10);
        assert_eq!(run.tokens_consumed, 200);
        assert!((run.model_cost - 0.50).abs() < 1e-12);
        assert!((run.tool_cost - 0.05).abs() < 1e-12);
        assert!((run.cost_estimate - 0.55).abs() < 1e-12);

        // The old entry point still records model usage.
        run.add_usage(10, 0.01);
        assert!((run.model_cost - 0.51).abs() < 1e-12);
    }
}

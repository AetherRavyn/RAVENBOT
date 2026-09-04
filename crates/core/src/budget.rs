use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use uuid::Uuid;

/// Budget limit type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BudgetLimit {
    /// No limit
    Unlimited,
    /// Maximum tokens per period
    Tokens { max: u64 },
    /// Maximum cost per period (in dollars)
    Cost { max: f64 },
}

/// Behavior when budget is hit
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BudgetBehavior {
    /// Hard stop - immediately halt the bot
    HardStop,
    /// Warn but continue
    Warn,
    /// Ask the user what to do
    AskUser,
}

/// Budget period
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BudgetPeriod {
    /// Per hour
    Hourly,
    /// Per day
    Daily,
    /// Per week
    Weekly,
    /// Per month
    Monthly,
    /// Total (lifetime)
    Total,
}

/// Budget configuration for a bot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Budget {
    /// Unique identifier
    pub id: Uuid,
    /// Bot this budget applies to
    pub bot_id: Uuid,
    /// Budget limit
    pub limit: BudgetLimit,
    /// Behavior when limit is hit
    pub behavior: BudgetBehavior,
    /// Reset period
    pub period: BudgetPeriod,
    /// Created timestamp
    pub created_at: DateTime<Utc>,
    /// Last updated timestamp
    pub updated_at: DateTime<Utc>,
}

impl Budget {
    /// Create a new budget
    pub fn new(bot_id: Uuid, limit: BudgetLimit, period: BudgetPeriod) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            bot_id,
            limit,
            behavior: BudgetBehavior::HardStop,
            period,
            created_at: now,
            updated_at: now,
        }
    }
}

/// Usage tracking for a budget period
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetUsage {
    /// Current tokens used in this period
    pub tokens_used: u64,
    /// Current cost in this period
    pub cost_used: f64,
    /// Period start timestamp
    pub period_start: DateTime<Utc>,
    /// Period end timestamp
    pub period_end: DateTime<Utc>,
}

impl BudgetUsage {
    /// Check if the budget is exceeded
    pub fn is_exceeded(&self, budget: &Budget) -> bool {
        match &budget.limit {
            BudgetLimit::Unlimited => false,
            BudgetLimit::Tokens { max } => self.tokens_used >= *max,
            BudgetLimit::Cost { max } => self.cost_used >= *max,
        }
    }

    /// Get remaining budget (returns None if unlimited)
    pub fn remaining(&self, budget: &Budget) -> Option<f64> {
        match &budget.limit {
            BudgetLimit::Unlimited => None,
            BudgetLimit::Tokens { max } => Some((*max).saturating_sub(self.tokens_used) as f64),
            BudgetLimit::Cost { max } => Some((max - self.cost_used).max(0.0)),
        }
    }
}

//! Budget management for bots

use ravenbot_core::{Budget, BudgetLimit, BudgetBehavior, BudgetPeriod};
use sqlx::SqlitePool;
use uuid::Uuid;

/// Budget manager for tracking and enforcing limits
pub struct BudgetManager {
    pool: SqlitePool,
}

impl BudgetManager {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Set a budget for a bot
    pub async fn set_budget(&self, budget: &Budget) -> Result<(), String> {
        let limit_type = match &budget.limit {
            BudgetLimit::Unlimited => "unlimited",
            BudgetLimit::Tokens { .. } => "tokens",
            BudgetLimit::Cost { .. } => "cost",
        };

        let limit_value = match &budget.limit {
            BudgetLimit::Unlimited => 0.0,
            BudgetLimit::Tokens { max } => *max as f64,
            BudgetLimit::Cost { max } => *max,
        };

        let behavior = match budget.behavior {
            BudgetBehavior::HardStop => "hard_stop",
            BudgetBehavior::Warn => "warn",
            BudgetBehavior::AskUser => "ask_user",
        };

        let period = match budget.period {
            BudgetPeriod::Hourly => "hourly",
            BudgetPeriod::Daily => "daily",
            BudgetPeriod::Weekly => "weekly",
            BudgetPeriod::Monthly => "monthly",
            BudgetPeriod::Total => "total",
        };

        sqlx::query(
            "INSERT OR REPLACE INTO budgets (id, bot_id, limit_type, limit_value, behavior, period, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(budget.id.to_string())
        .bind(budget.bot_id.to_string())
        .bind(limit_type)
        .bind(limit_value)
        .bind(behavior)
        .bind(period)
        .bind(budget.created_at.to_rfc3339())
        .bind(budget.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        tracing::info!(bot_id = %budget.bot_id, "Budget set");
        Ok(())
    }

    /// Get budget for a bot
    pub async fn get_budget(&self, bot_id: Uuid) -> Result<Option<Budget>, String> {
        let row: Option<(String, String, String, f64, String, String, String, String)> = sqlx::query_as(
            "SELECT id, bot_id, limit_type, limit_value, behavior, period, created_at, updated_at
             FROM budgets WHERE bot_id = ?"
        )
        .bind(bot_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        match row {
            Some(row) => {
                let limit = match row.2.as_str() {
                    "unlimited" => BudgetLimit::Unlimited,
                    "tokens" => BudgetLimit::Tokens { max: row.3 as u64 },
                    "cost" => BudgetLimit::Cost { max: row.3 },
                    _ => BudgetLimit::Unlimited,
                };

                let behavior = match row.4.as_str() {
                    "hard_stop" => BudgetBehavior::HardStop,
                    "warn" => BudgetBehavior::Warn,
                    "ask_user" => BudgetBehavior::AskUser,
                    _ => BudgetBehavior::HardStop,
                };

                let period = match row.5.as_str() {
                    "hourly" => BudgetPeriod::Hourly,
                    "daily" => BudgetPeriod::Daily,
                    "weekly" => BudgetPeriod::Weekly,
                    "monthly" => BudgetPeriod::Monthly,
                    "total" => BudgetPeriod::Total,
                    _ => BudgetPeriod::Total,
                };

                Ok(Some(Budget {
                    id: Uuid::parse_str(&row.0).unwrap_or_default(),
                    bot_id,
                    limit,
                    behavior,
                    period,
                    created_at: chrono::DateTime::parse_from_rfc3339(&row.6)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now()),
                    updated_at: chrono::DateTime::parse_from_rfc3339(&row.7)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now()),
                }))
            }
            None => Ok(None),
        }
    }

    /// Record usage for a bot
    pub async fn record_usage(&self, bot_id: Uuid, tokens: u64, cost: f64) -> Result<(), String> {
        // Real usage accounting: accumulate into budget_usage (migration 008)
        sqlx::query(
            r#"INSERT INTO budget_usage (bot_id, tokens_used, cost_used, updated_at)
               VALUES (?, ?, ?, ?)
               ON CONFLICT(bot_id) DO UPDATE SET
                   tokens_used = tokens_used + excluded.tokens_used,
                   cost_used = cost_used + excluded.cost_used,
                   updated_at = excluded.updated_at"#,
        )
        .bind(bot_id.to_string())
        .bind(tokens as i64)
        .bind(cost)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Current accumulated usage for a bot
    pub async fn get_usage(&self, bot_id: Uuid) -> Result<(u64, f64), String> {
        let row: Option<(i64, f64)> =
            sqlx::query_as("SELECT tokens_used, cost_used FROM budget_usage WHERE bot_id = ?")
                .bind(bot_id.to_string())
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| e.to_string())?;
        Ok(row.map(|(t, c)| (t as u64, c)).unwrap_or((0, 0.0)))
    }

    /// Reset accumulated usage for a bot (period rollover / manual reset)
    pub async fn reset_usage(&self, bot_id: Uuid) -> Result<(), String> {
        sqlx::query("DELETE FROM budget_usage WHERE bot_id = ?")
            .bind(bot_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Check if budget is exceeded
    pub async fn check_budget(&self, bot_id: Uuid) -> Result<BudgetCheck, String> {
        let budget = self.get_budget(bot_id).await?;

        match budget {
            Some(budget) => {
                match &budget.limit {
                    BudgetLimit::Unlimited => Ok(BudgetCheck {
                        allowed: true,
                        remaining: None,
                        percentage_used: 0.0,
                        should_warn: false,
                    }),
                    BudgetLimit::Tokens { max } => {
                        let (used, _) = self.get_usage(bot_id).await?;
                        let remaining = max.saturating_sub(used);
                        let percentage = if *max > 0 { (used as f64 / *max as f64) * 100.0 } else { 100.0 };

                        Ok(BudgetCheck {
                            allowed: remaining > 0,
                            remaining: Some(remaining as f64),
                            percentage_used: percentage,
                            should_warn: percentage > 80.0,
                        })
                    }
                    BudgetLimit::Cost { max } => {
                        let (_, used) = self.get_usage(bot_id).await?;
                        let remaining = (max - used).max(0.0);
                        let percentage = if *max > 0.0 { (used / *max) * 100.0 } else { 100.0 };

                        Ok(BudgetCheck {
                            allowed: remaining > 0.0,
                            remaining: Some(remaining),
                            percentage_used: percentage,
                            should_warn: percentage > 80.0,
                        })
                    }
                }
            }
            None => Ok(BudgetCheck {
                allowed: true,
                remaining: None,
                percentage_used: 0.0,
                should_warn: false,
            }),
        }
    }
}

/// Result of a budget check
#[derive(Debug, Clone, serde::Serialize)]
pub struct BudgetCheck {
    /// Whether the action is allowed
    pub allowed: bool,
    /// Remaining budget (None if unlimited)
    pub remaining: Option<f64>,
    /// Percentage of budget used
    pub percentage_used: f64,
    /// Whether to warn the user
    pub should_warn: bool,
}

//! Budget management for bots

use chrono::{DateTime, Datelike, Utc};
use ravenbot_core::{Budget, BudgetBehavior, BudgetLimit, BudgetPeriod};
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

    /// A stable identifier for the current usage window. Changing periods
    /// starts a fresh window rather than carrying old spend into the new one.
    fn current_period_key(period: &BudgetPeriod, now: DateTime<Utc>) -> String {
        match period {
            BudgetPeriod::Hourly => now.format("%Y-%m-%dT%H").to_string(),
            BudgetPeriod::Daily => now.format("%Y-%m-%d").to_string(),
            BudgetPeriod::Weekly => {
                let week = now.iso_week();
                format!("{}-W{:02}", week.year(), week.week())
            }
            BudgetPeriod::Monthly => now.format("%Y-%m").to_string(),
            BudgetPeriod::Total => "total".to_string(),
        }
    }

    /// Set a budget for a bot
    pub async fn set_budget(&self, budget: &Budget) -> Result<(), String> {
        let previous = self.get_budget(budget.bot_id).await?;
        if previous
            .as_ref()
            .is_some_and(|prior| prior.period != budget.period)
        {
            self.reset_usage(budget.bot_id).await?;
        }
        // Reuse the existing row. Minting a new id here used to create a second
        // budget row on every save, leaving the first one active forever.
        let id = previous.as_ref().map(|prior| prior.id).unwrap_or(budget.id);

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
        .bind(id.to_string())
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
             FROM budgets WHERE bot_id = ? ORDER BY updated_at DESC LIMIT 1"
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

    /// Record usage for a bot in its current budget window.
    pub async fn record_usage(&self, bot_id: Uuid, tokens: u64, cost: f64) -> Result<(), String> {
        let period = self
            .get_budget(bot_id)
            .await?
            .map(|budget| budget.period)
            .unwrap_or(BudgetPeriod::Total);
        let key = Self::current_period_key(&period, Utc::now());
        // The CASE expressions make rollover atomic with the write: a row
        // from an older window is replaced, not incremented.
        sqlx::query(
            r#"INSERT INTO budget_usage
                   (bot_id, tokens_used, cost_used, period_key, total_tokens, total_cost, updated_at)
               VALUES (?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(bot_id) DO UPDATE SET
                   tokens_used = CASE WHEN budget_usage.period_key = excluded.period_key
                       THEN budget_usage.tokens_used + excluded.tokens_used
                       ELSE excluded.tokens_used END,
                   cost_used = CASE WHEN budget_usage.period_key = excluded.period_key
                       THEN budget_usage.cost_used + excluded.cost_used
                       ELSE excluded.cost_used END,
                   period_key = excluded.period_key,
                   total_tokens = budget_usage.total_tokens + excluded.total_tokens,
                   total_cost = budget_usage.total_cost + excluded.total_cost,
                   updated_at = excluded.updated_at"#,
        )
        .bind(bot_id.to_string())
        .bind(tokens as i64)
        .bind(cost)
        .bind(&key)
        .bind(tokens as i64)
        .bind(cost)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Usage in the bot's current budget window. A stale window reads as zero
    /// and is reset, so an old month's spend cannot block a new month.
    pub async fn get_usage(&self, bot_id: Uuid) -> Result<(u64, f64), String> {
        let period = self
            .get_budget(bot_id)
            .await?
            .map(|budget| budget.period)
            .unwrap_or(BudgetPeriod::Total);
        let key = Self::current_period_key(&period, Utc::now());
        let row: Option<(i64, f64, String)> = sqlx::query_as(
            "SELECT tokens_used, cost_used, period_key FROM budget_usage WHERE bot_id = ?",
        )
        .bind(bot_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        match row {
            Some((tokens, cost, stored)) if stored == key => Ok((tokens as u64, cost)),
            Some(_) => {
                sqlx::query(
                    "UPDATE budget_usage SET tokens_used = 0, cost_used = 0,
                        period_key = ?, updated_at = ? WHERE bot_id = ?",
                )
                .bind(&key)
                .bind(Utc::now().to_rfc3339())
                .bind(bot_id.to_string())
                .execute(&self.pool)
                .await
                .map_err(|e| e.to_string())?;
                Ok((0, 0.0))
            }
            None => Ok((0, 0.0)),
        }
    }

    /// Reset the current window while keeping lifetime totals for telemetry
    /// and office baselines.
    pub async fn reset_usage(&self, bot_id: Uuid) -> Result<(), String> {
        sqlx::query(
            "UPDATE budget_usage SET tokens_used = 0, cost_used = 0,
                period_key = '', updated_at = ? WHERE bot_id = ?",
        )
        .bind(Utc::now().to_rfc3339())
        .bind(bot_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Lifetime usage, independent of the active budget window.
    pub async fn get_total_usage(&self, bot_id: Uuid) -> Result<(u64, f64), String> {
        let row: Option<(i64, f64)> =
            sqlx::query_as("SELECT total_tokens, total_cost FROM budget_usage WHERE bot_id = ?")
                .bind(bot_id.to_string())
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| e.to_string())?;
        Ok(row
            .map(|(tokens, cost)| (tokens as u64, cost))
            .unwrap_or((0, 0.0)))
    }

    /// Check if budget is exceeded
    pub async fn check_budget(&self, bot_id: Uuid) -> Result<BudgetCheck, String> {
        let budget = self.get_budget(bot_id).await?;

        match budget {
            Some(budget) => match &budget.limit {
                BudgetLimit::Unlimited => Ok(BudgetCheck {
                    allowed: true,
                    remaining: None,
                    percentage_used: 0.0,
                    should_warn: false,
                }),
                BudgetLimit::Tokens { max } => {
                    let (used, _) = self.get_usage(bot_id).await?;
                    let remaining = max.saturating_sub(used);
                    let percentage = if *max > 0 {
                        (used as f64 / *max as f64) * 100.0
                    } else {
                        100.0
                    };
                    let hard_stop = matches!(
                        budget.behavior,
                        BudgetBehavior::HardStop | BudgetBehavior::AskUser
                    );

                    Ok(BudgetCheck {
                        allowed: !hard_stop || remaining > 0,
                        remaining: Some(remaining as f64),
                        percentage_used: percentage,
                        should_warn: percentage >= 80.0,
                    })
                }
                BudgetLimit::Cost { max } => {
                    let (_, used) = self.get_usage(bot_id).await?;
                    let remaining = (max - used).max(0.0);
                    let percentage = if *max > 0.0 {
                        (used / *max) * 100.0
                    } else {
                        100.0
                    };
                    let hard_stop = matches!(
                        budget.behavior,
                        BudgetBehavior::HardStop | BudgetBehavior::AskUser
                    );

                    Ok(BudgetCheck {
                        allowed: !hard_stop || remaining > 0.0,
                        remaining: Some(remaining),
                        percentage_used: percentage,
                        should_warn: percentage >= 80.0,
                    })
                }
            },
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

#[cfg(test)]
mod tests {
    use super::*;
    use ravenbot_core::Bot;

    async fn temp_db() -> ravenbot_db::Database {
        let path =
            std::env::temp_dir().join(format!("ravenbot-budget-window-{}.db", Uuid::new_v4()));
        ravenbot_db::Database::new(&path).await.expect("temp db")
    }

    #[test]
    fn period_keys_follow_the_calendar() {
        let now = DateTime::parse_from_rfc3339("2026-11-17T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(
            BudgetManager::current_period_key(&BudgetPeriod::Hourly, now),
            "2026-11-17T10"
        );
        assert_eq!(
            BudgetManager::current_period_key(&BudgetPeriod::Daily, now),
            "2026-11-17"
        );
        assert_eq!(
            BudgetManager::current_period_key(&BudgetPeriod::Weekly, now),
            "2026-W47"
        );
        assert_eq!(
            BudgetManager::current_period_key(&BudgetPeriod::Monthly, now),
            "2026-11"
        );
        assert_eq!(
            BudgetManager::current_period_key(&BudgetPeriod::Total, now),
            "total"
        );
    }

    #[tokio::test]
    async fn daily_usage_resets_when_the_window_moves() {
        let db = temp_db().await;
        let bot = Bot::new("Budgeted", "daily budget test");
        ravenbot_db::queries::BotQueries::insert(db.pool(), &bot)
            .await
            .unwrap();
        let budgets = BudgetManager::new(db.pool().clone());
        budgets
            .set_budget(&Budget::new(
                bot.id,
                BudgetLimit::Tokens { max: 100 },
                BudgetPeriod::Daily,
            ))
            .await
            .unwrap();

        budgets.record_usage(bot.id, 40, 0.5).await.unwrap();
        assert_eq!(budgets.get_usage(bot.id).await.unwrap(), (40, 0.5));
        assert_eq!(budgets.get_total_usage(bot.id).await.unwrap(), (40, 0.5));

        // Simulate yesterday's row without waiting for midnight.
        sqlx::query("UPDATE budget_usage SET period_key = '1999-01-01' WHERE bot_id = ?")
            .bind(bot.id.to_string())
            .execute(db.pool())
            .await
            .unwrap();
        assert_eq!(budgets.get_usage(bot.id).await.unwrap(), (0, 0.0));
        let check = budgets.check_budget(bot.id).await.unwrap();
        assert!(check.allowed, "a new day starts with a fresh budget");

        budgets.record_usage(bot.id, 10, 0.1).await.unwrap();
        assert_eq!(budgets.get_usage(bot.id).await.unwrap(), (10, 0.1));
        assert_eq!(
            budgets.get_total_usage(bot.id).await.unwrap(),
            (50, 0.6),
            "a window rollover must not erase lifetime telemetry"
        );

        budgets.reset_usage(bot.id).await.unwrap();
        assert_eq!(budgets.get_usage(bot.id).await.unwrap(), (0, 0.0));
        assert_eq!(
            budgets.get_total_usage(bot.id).await.unwrap(),
            (50, 0.6),
            "resetting a budget is not a history reset"
        );
    }

    #[tokio::test]
    async fn updating_a_budget_reuses_its_row_and_applies_new_settings() {
        let db = temp_db().await;
        let bot = Bot::new("Budgeted", "single row test");
        ravenbot_db::queries::BotQueries::insert(db.pool(), &bot)
            .await
            .unwrap();
        let budgets = BudgetManager::new(db.pool().clone());

        budgets
            .set_budget(&Budget::new(
                bot.id,
                BudgetLimit::Tokens { max: 100 },
                BudgetPeriod::Total,
            ))
            .await
            .unwrap();
        let mut updated = Budget::new(
            bot.id,
            BudgetLimit::Cost { max: 2.5 },
            BudgetPeriod::Monthly,
        );
        updated.behavior = BudgetBehavior::Warn;
        budgets.set_budget(&updated).await.unwrap();

        let stored = budgets.get_budget(bot.id).await.unwrap().expect("budget");
        assert_eq!(stored.behavior, BudgetBehavior::Warn);
        assert_eq!(stored.limit, BudgetLimit::Cost { max: 2.5 });
        assert_eq!(stored.period, BudgetPeriod::Monthly);
        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM budgets WHERE bot_id = ?")
            .bind(bot.id.to_string())
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(rows, 1, "a bot must have exactly one budget row");
    }

    #[tokio::test]
    async fn warn_behavior_allows_overage_while_hard_stop_refuses() {
        let db = temp_db().await;
        let bot = Bot::new("Budgeted", "behavior test");
        ravenbot_db::queries::BotQueries::insert(db.pool(), &bot)
            .await
            .unwrap();
        let budgets = BudgetManager::new(db.pool().clone());

        let mut warn = Budget::new(
            bot.id,
            BudgetLimit::Tokens { max: 100 },
            BudgetPeriod::Total,
        );
        warn.behavior = BudgetBehavior::Warn;
        budgets.set_budget(&warn).await.unwrap();
        budgets.record_usage(bot.id, 120, 0.0).await.unwrap();
        let check = budgets.check_budget(bot.id).await.unwrap();
        assert!(check.allowed, "Warn must continue past the limit");
        assert!(check.should_warn);
        assert!(check.percentage_used >= 120.0);

        let hard = Budget::new(
            bot.id,
            BudgetLimit::Tokens { max: 100 },
            BudgetPeriod::Total,
        );
        budgets.set_budget(&hard).await.unwrap();
        let check = budgets.check_budget(bot.id).await.unwrap();
        assert!(!check.allowed, "HardStop must refuse past the limit");
    }

    #[tokio::test]
    async fn changing_the_period_starts_a_fresh_window() {
        let db = temp_db().await;
        let bot = Bot::new("Budgeted", "period change test");
        ravenbot_db::queries::BotQueries::insert(db.pool(), &bot)
            .await
            .unwrap();
        let budgets = BudgetManager::new(db.pool().clone());
        budgets
            .set_budget(&Budget::new(
                bot.id,
                BudgetLimit::Cost { max: 1.0 },
                BudgetPeriod::Total,
            ))
            .await
            .unwrap();
        budgets.record_usage(bot.id, 10, 0.4).await.unwrap();
        assert_eq!(budgets.get_usage(bot.id).await.unwrap(), (10, 0.4));

        budgets
            .set_budget(&Budget::new(
                bot.id,
                BudgetLimit::Cost { max: 1.0 },
                BudgetPeriod::Monthly,
            ))
            .await
            .unwrap();
        assert_eq!(budgets.get_usage(bot.id).await.unwrap(), (0, 0.0));
        assert_eq!(
            budgets.get_total_usage(bot.id).await.unwrap(),
            (10, 0.4),
            "changing the period keeps lifetime telemetry"
        );
    }
}

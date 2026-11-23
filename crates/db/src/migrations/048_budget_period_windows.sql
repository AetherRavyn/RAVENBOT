-- Budget usage windows. `period_key` identifies the window the accumulated
-- totals belong to, and a new key starts fresh. Lifetime totals are kept
-- separately so telemetry and office baselines survive a window rollover.
ALTER TABLE budget_usage ADD COLUMN period_key TEXT NOT NULL DEFAULT 'total';
ALTER TABLE budget_usage ADD COLUMN total_tokens INTEGER NOT NULL DEFAULT 0;
ALTER TABLE budget_usage ADD COLUMN total_cost REAL NOT NULL DEFAULT 0;
UPDATE budget_usage SET total_tokens = tokens_used, total_cost = cost_used;

CREATE INDEX IF NOT EXISTS idx_budget_usage_period
    ON budget_usage(bot_id, period_key);

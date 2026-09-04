-- Budget usage tracking: real token/cost accounting per bot
CREATE TABLE IF NOT EXISTS budget_usage (
    bot_id TEXT PRIMARY KEY REFERENCES bots(id) ON DELETE CASCADE,
    tokens_used INTEGER NOT NULL DEFAULT 0,
    cost_used REAL NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

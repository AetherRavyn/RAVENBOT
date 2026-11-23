-- Split run spend into model and tool components, and give utility model calls
-- a ledger of their own.
--
-- Historical runs only recorded a single total. They are backfilled as model
-- spend because the old tool charges were folded into the same number and
-- cannot be separated after the fact. New runs write both buckets.
ALTER TABLE runs ADD COLUMN model_cost REAL NOT NULL DEFAULT 0;
ALTER TABLE runs ADD COLUMN tool_cost REAL NOT NULL DEFAULT 0;
UPDATE runs SET model_cost = cost_estimate WHERE model_cost = 0 AND cost_estimate > 0;

-- Planning, synthesis and org-design calls happen outside an interactive run.
-- They still spend real money, so they get a row here instead of disappearing
-- into the budget counter.
CREATE TABLE IF NOT EXISTS utility_usage (
    id TEXT PRIMARY KEY NOT NULL,
    bot_id TEXT NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    tokens INTEGER NOT NULL DEFAULT 0,
    cost REAL NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_utility_usage_bot_created
    ON utility_usage(bot_id, created_at);

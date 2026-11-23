-- A bot has one budget. Earlier writes inserted a new row on every save, so
-- reads kept returning the oldest budget and edits appeared to do nothing.
-- Keep the newest row per bot, then enforce it at the schema level.
DELETE FROM budgets
WHERE rowid NOT IN (SELECT MAX(rowid) FROM budgets GROUP BY bot_id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_budgets_bot_unique ON budgets(bot_id);

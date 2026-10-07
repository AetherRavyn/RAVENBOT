-- Per-agent file-change journal.
--
-- "Which agent changed what, and by how much" was unanswerable: tool calls were
-- transient events in the browser, so after a reload there was no record that
-- any file had been touched, let alone by whom. An office of five agents could
-- rewrite a project and leave nothing behind that said who did which part.
--
-- One row per write rather than a running total per bot, because a total cannot
-- be asked questions: "what did Sam change before the build broke", "revert
-- everything Priya did this run", "was this file written twice". Totals are a
-- query over this table, not a column on it.
--
-- `path` is the path the tool resolved to, which is what the agent saw when it
-- wrote. Relative where the tool was given a relative path, because that is what
-- the agent's own output says and what a reader will search for.
CREATE TABLE IF NOT EXISTS file_changes (
    id TEXT PRIMARY KEY,
    bot_id TEXT NOT NULL,
    run_id TEXT,
    thread_id TEXT,
    path TEXT NOT NULL,
    skill TEXT NOT NULL,
    lines_added INTEGER NOT NULL DEFAULT 0,
    lines_deleted INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- The two queries this exists for: totals for one agent's list, and the recent
-- list for one agent. `bot_id` leads both.
CREATE INDEX IF NOT EXISTS idx_file_changes_bot ON file_changes (bot_id, created_at);
CREATE INDEX IF NOT EXISTS idx_file_changes_run ON file_changes (run_id);

-- Human-in-the-loop questions (ask_user tool) + run cancellation support
-- Version 13

-- Pending questions: one row per parked ask_user call. The transcript card,
-- the answer path, and the resume logic all read this table.
CREATE TABLE IF NOT EXISTS questions (
    id TEXT PRIMARY KEY NOT NULL,
    bot_id TEXT NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    run_id TEXT NOT NULL,
    header TEXT NOT NULL DEFAULT 'Question',
    question TEXT NOT NULL,
    options TEXT NOT NULL DEFAULT '[]',
    allow_custom INTEGER NOT NULL DEFAULT 1,
    status TEXT NOT NULL DEFAULT 'pending',
    answer TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    answered_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_questions_thread ON questions(thread_id, status);
CREATE INDEX IF NOT EXISTS idx_questions_run ON questions(run_id, status);
CREATE INDEX IF NOT EXISTS idx_questions_bot ON questions(bot_id, status);

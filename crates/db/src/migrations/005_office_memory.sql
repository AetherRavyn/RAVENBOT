-- Office shared memory + agent learning to get smarter daily
CREATE TABLE IF NOT EXISTS office_memories (
    id TEXT PRIMARY KEY NOT NULL,
    chatroom_id TEXT NOT NULL REFERENCES chatrooms(id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    embedding BLOB,
    importance REAL NOT NULL DEFAULT 0.5,
    category TEXT NOT NULL DEFAULT 'general',
    created_by TEXT,
    access_count INTEGER NOT NULL DEFAULT 0,
    last_accessed TEXT NOT NULL DEFAULT (datetime('now')),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_office_memories_chatroom ON office_memories(chatroom_id);
CREATE INDEX IF NOT EXISTS idx_office_memories_importance ON office_memories(importance DESC);

-- Agent learning & intelligence tracking (makes agents smarter day by day)
CREATE TABLE IF NOT EXISTS agent_learnings (
    id TEXT PRIMARY KEY NOT NULL,
    bot_id TEXT NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    chatroom_id TEXT REFERENCES chatrooms(id) ON DELETE CASCADE,
    learning_type TEXT NOT NULL, -- success, failure, preference, skill
    content TEXT NOT NULL,
    context TEXT,
    success_rate REAL NOT NULL DEFAULT 0.5,
    tasks_completed INTEGER NOT NULL DEFAULT 0,
    tasks_failed INTEGER NOT NULL DEFAULT 0,
    avg_response_time_ms INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_agent_learnings_bot ON agent_learnings(bot_id);
CREATE INDEX IF NOT EXISTS idx_agent_learnings_chatroom ON agent_learnings(chatroom_id);

-- Daily intelligence snapshot per bot (for smarter trends)
CREATE TABLE IF NOT EXISTS agent_intelligence (
    bot_id TEXT PRIMARY KEY NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    total_memories INTEGER NOT NULL DEFAULT 0,
    office_memories INTEGER NOT NULL DEFAULT 0,
    learnings_count INTEGER NOT NULL DEFAULT 0,
    intelligence_score REAL NOT NULL DEFAULT 0.5,
    tasks_today INTEGER NOT NULL DEFAULT 0,
    success_streak INTEGER NOT NULL DEFAULT 0,
    last_active TEXT,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

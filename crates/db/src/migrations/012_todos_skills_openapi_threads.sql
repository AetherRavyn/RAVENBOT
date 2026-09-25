-- Foundations: todo persistence, skill stats, OpenAPI ops, thread flags
-- Version 12

-- Per-bot todos (replaces the process-memory static map — survives restarts)
CREATE TABLE IF NOT EXISTS bot_todos (
    id TEXT PRIMARY KEY NOT NULL,
    bot_id TEXT NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    task TEXT NOT NULL,
    done INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_bot_todos_bot ON bot_todos(bot_id, done);

-- Per-bot per-skill outcomes (proficiency computed from real data)
CREATE TABLE IF NOT EXISTS skill_stats (
    bot_id TEXT NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    skill_id TEXT NOT NULL,
    ok_count INTEGER NOT NULL DEFAULT 0,
    err_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (bot_id, skill_id)
);

-- Parsed OpenAPI operations (import creates usable skills, not raw text)
CREATE TABLE IF NOT EXISTS openapi_operations (
    id TEXT PRIMARY KEY NOT NULL,
    plugin_id TEXT NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    operation_id TEXT NOT NULL,
    method TEXT NOT NULL,
    path_template TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    input_schema TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_openapi_ops_plugin ON openapi_operations(plugin_id);

-- Thread roster flags (pin / unread / hide)
ALTER TABLE threads ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;
ALTER TABLE threads ADD COLUMN last_read_at TEXT;
ALTER TABLE bots ADD COLUMN hidden INTEGER NOT NULL DEFAULT 0;

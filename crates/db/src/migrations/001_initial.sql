-- RAVENBOT Initial Schema
-- Version 1

-- Bots table
CREATE TABLE IF NOT EXISTS bots (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    avatar_color TEXT NOT NULL DEFAULT '#6366f1',
    status TEXT NOT NULL DEFAULT 'idle',
    config TEXT NOT NULL DEFAULT '{}',
    permissions TEXT NOT NULL DEFAULT '[]',
    is_orchestrator INTEGER NOT NULL DEFAULT 0,
    delegate_to TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_active_at TEXT
);

-- Threads table
CREATE TABLE IF NOT EXISTS threads (
    id TEXT PRIMARY KEY NOT NULL,
    bot_id TEXT NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_threads_bot_id ON threads(bot_id);

-- Messages table
CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY NOT NULL,
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    attachments TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_messages_thread_id ON messages(thread_id);

-- Bot skills junction table
CREATE TABLE IF NOT EXISTS bot_skills (
    bot_id TEXT NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    skill_id TEXT NOT NULL,
    PRIMARY KEY (bot_id, skill_id)
);

-- Memory facts table
CREATE TABLE IF NOT EXISTS memory_facts (
    id TEXT PRIMARY KEY NOT NULL,
    bot_id TEXT NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    embedding BLOB,
    importance REAL NOT NULL DEFAULT 0.5,
    access_count INTEGER NOT NULL DEFAULT 0,
    last_accessed TEXT NOT NULL DEFAULT (datetime('now')),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_memory_facts_bot_id ON memory_facts(bot_id);

-- Routines table
CREATE TABLE IF NOT EXISTS routines (
    id TEXT PRIMARY KEY NOT NULL,
    bot_id TEXT NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    schedule TEXT NOT NULL,
    instruction TEXT NOT NULL,
    is_enabled INTEGER NOT NULL DEFAULT 1,
    last_run_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_routines_bot_id ON routines(bot_id);

-- Skill registry table
CREATE TABLE IF NOT EXISTS skill_registry (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    version TEXT NOT NULL DEFAULT '1.0.0',
    required_permissions TEXT NOT NULL DEFAULT '[]',
    input_schema TEXT NOT NULL DEFAULT '{}',
    is_builtin INTEGER NOT NULL DEFAULT 0,
    is_signed INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Bot versions table (for prompt diff/rollback)
CREATE TABLE IF NOT EXISTS bot_versions (
    id TEXT PRIMARY KEY NOT NULL,
    bot_id TEXT NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    version_number INTEGER NOT NULL,
    system_prompt TEXT NOT NULL,
    config TEXT NOT NULL DEFAULT '{}',
    source TEXT NOT NULL DEFAULT 'user',
    description TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_bot_versions_bot_id ON bot_versions(bot_id);

-- Runs table (with checkpoint for crash recovery)
CREATE TABLE IF NOT EXISTS runs (
    id TEXT PRIMARY KEY NOT NULL,
    bot_id TEXT NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    parent_run_id TEXT REFERENCES runs(id),
    state TEXT NOT NULL DEFAULT 'planning',
    checkpoint TEXT,
    outcome TEXT,
    tokens_consumed INTEGER NOT NULL DEFAULT 0,
    cost_estimate REAL NOT NULL DEFAULT 0.0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT
);

CREATE INDEX idx_runs_bot_id ON runs(bot_id);
CREATE INDEX idx_runs_thread_id ON runs(thread_id);
CREATE INDEX idx_runs_parent_run_id ON runs(parent_run_id);

-- Budgets table
CREATE TABLE IF NOT EXISTS budgets (
    id TEXT PRIMARY KEY NOT NULL,
    bot_id TEXT NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    limit_type TEXT NOT NULL,
    limit_value REAL NOT NULL DEFAULT 0.0,
    behavior TEXT NOT NULL DEFAULT 'hard_stop',
    period TEXT NOT NULL DEFAULT 'total',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_budgets_bot_id ON budgets(bot_id);

-- Audit log table (append-only)
CREATE TABLE IF NOT EXISTS audit_log (
    id TEXT PRIMARY KEY NOT NULL,
    bot_id TEXT NOT NULL,
    run_id TEXT,
    thread_id TEXT,
    event TEXT NOT NULL,
    timestamp TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_audit_log_bot_id ON audit_log(bot_id);
CREATE INDEX idx_audit_log_timestamp ON audit_log(timestamp);

-- Bot bundles table (for export/import)
CREATE TABLE IF NOT EXISTS bot_bundles (
    id TEXT PRIMARY KEY NOT NULL,
    bot_id TEXT NOT NULL,
    bundle_data TEXT NOT NULL,
    signature TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

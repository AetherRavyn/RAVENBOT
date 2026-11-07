-- Team channels: a shared timeline and tasks with exactly one active owner.
--
-- Channels (migration 014) already hold the roster, instructions, and folder.
-- This adds the record people and agents return to: messages, tasks, and the
-- transition history that explains an ownership change.

CREATE TABLE IF NOT EXISTS channel_messages (
    id TEXT PRIMARY KEY NOT NULL,
    channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    author_kind TEXT NOT NULL DEFAULT 'human',
    author_id TEXT,
    author_name TEXT NOT NULL DEFAULT '',
    content TEXT NOT NULL,
    reply_to_id TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_channel_messages_channel
    ON channel_messages(channel_id, created_at);

CREATE TABLE IF NOT EXISTS channel_tasks (
    id TEXT PRIMARY KEY NOT NULL,
    channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    owner_bot_id TEXT NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    state TEXT NOT NULL DEFAULT 'pending',
    created_by TEXT NOT NULL DEFAULT '',
    source_thread_id TEXT,
    result TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    archived_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_channel_tasks_channel
    ON channel_tasks(channel_id, state, created_at);

CREATE TABLE IF NOT EXISTS channel_task_events (
    id TEXT PRIMARY KEY NOT NULL,
    task_id TEXT NOT NULL REFERENCES channel_tasks(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    actor TEXT NOT NULL DEFAULT '',
    from_owner TEXT,
    to_owner TEXT,
    note TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_channel_task_events_task
    ON channel_task_events(task_id, created_at);

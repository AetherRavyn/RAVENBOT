-- Durable office graph snapshots.
--
-- An office turn is a DAG. The graph used to live only in memory, so a restart
-- had to replan the whole turn and could repeat completed work. This table is
-- the graph's durable projection: one row per cycle, updated while the graph
-- runs and marked terminal when it finishes. A `running` row is resumable.
CREATE TABLE IF NOT EXISTS office_graphs (
    id TEXT PRIMARY KEY NOT NULL,
    chatroom_id TEXT NOT NULL REFERENCES chatrooms(id) ON DELETE CASCADE,
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    cycle INTEGER NOT NULL DEFAULT 0,
    request TEXT NOT NULL DEFAULT '',
    visible TEXT NOT NULL DEFAULT '',
    goal TEXT NOT NULL DEFAULT '',
    labels TEXT NOT NULL DEFAULT '{}',
    graph TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'running', -- running | completed | failed | abandoned
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_office_graphs_room_status
    ON office_graphs(chatroom_id, status, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_office_graphs_thread_status
    ON office_graphs(thread_id, status, updated_at DESC);

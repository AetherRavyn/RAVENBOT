-- FIFO user messages for an office turn that is already running.
-- This is deliberately separate from `messages`: a queued message becomes a
-- visible user message only when its turn starts, so the conversation order
-- matches execution order and a cancelled item leaves no orphan prompt.
CREATE TABLE IF NOT EXISTS office_queue (
    id TEXT PRIMARY KEY NOT NULL,
    chatroom_id TEXT NOT NULL REFERENCES chatrooms(id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    visible TEXT NOT NULL,
    attachments TEXT NOT NULL DEFAULT '[]',
    state TEXT NOT NULL DEFAULT 'pending', -- pending | running | done | failed | cancelled
    error TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    started_at TEXT,
    finished_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_office_queue_room_state
    ON office_queue(chatroom_id, state, created_at);

-- Persisted message reactions.
--
-- Reactions belong to the message they were placed on, but the message id can
-- refer to either a conversation message or a shared-channel message, and
-- SQLite cannot express a foreign key to "one of two tables". The delete paths
-- remove both the message and its reactions explicitly. The primary key is what
-- makes toggling a reaction idempotent instead of accumulating duplicates.
CREATE TABLE IF NOT EXISTS message_reactions (
    message_id TEXT NOT NULL,
    emoji TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (message_id, emoji)
);

CREATE INDEX IF NOT EXISTS idx_message_reactions_message
    ON message_reactions(message_id);

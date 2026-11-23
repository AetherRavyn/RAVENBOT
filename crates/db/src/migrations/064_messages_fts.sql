-- Full-text search over the transcript.
--
-- Messages are stored as JSON text in `messages.content`. A standalone FTS5
-- table (rather than external-content) keeps the message's text id and thread
-- id alongside the searchable body, so a hit can be opened without joining on
-- a TEXT primary key. Triggers keep it in sync; the backfill is idempotent.
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
    message_id UNINDEXED,
    thread_id UNINDEXED,
    content
);

CREATE TRIGGER IF NOT EXISTS messages_fts_after_insert
AFTER INSERT ON messages
BEGIN
    INSERT INTO messages_fts(message_id, thread_id, content)
    VALUES (new.id, new.thread_id, new.content);
END;

CREATE TRIGGER IF NOT EXISTS messages_fts_after_delete
AFTER DELETE ON messages
BEGIN
    DELETE FROM messages_fts WHERE message_id = old.id;
END;

CREATE TRIGGER IF NOT EXISTS messages_fts_after_update
AFTER UPDATE OF content, thread_id ON messages
BEGIN
    DELETE FROM messages_fts WHERE message_id = old.id;
    INSERT INTO messages_fts(message_id, thread_id, content)
    VALUES (new.id, new.thread_id, new.content);
END;

INSERT INTO messages_fts(message_id, thread_id, content)
SELECT m.id, m.thread_id, m.content
FROM messages m
WHERE NOT EXISTS (
    SELECT 1 FROM messages_fts f WHERE f.message_id = m.id
);

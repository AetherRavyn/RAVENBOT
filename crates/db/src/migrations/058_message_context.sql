-- Context usage belongs to the turn that produced the assistant message.
-- Stored on the message so the composer's meter can be restored on reload,
-- before the next run emits a live context event.
ALTER TABLE messages ADD COLUMN context_used_tokens INTEGER;
ALTER TABLE messages ADD COLUMN context_window_tokens INTEGER;
ALTER TABLE messages ADD COLUMN context_compacted INTEGER NOT NULL DEFAULT 0;

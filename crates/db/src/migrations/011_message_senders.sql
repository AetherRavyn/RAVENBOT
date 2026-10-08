-- Office messaging: sender attribution + replies (migration 011)
-- Messages in office group threads need to know WHICH bot spoke (role is
-- "assistant" for all of them) and which message they reply to.

ALTER TABLE messages ADD COLUMN sender_bot_id TEXT;
ALTER TABLE messages ADD COLUMN sender_name TEXT;
ALTER TABLE messages ADD COLUMN reply_to_id TEXT;

CREATE INDEX IF NOT EXISTS idx_messages_sender ON messages(sender_bot_id);

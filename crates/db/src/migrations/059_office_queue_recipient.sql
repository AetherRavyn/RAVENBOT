-- Preserve an addressed specialist when the message has to wait in the queue.
ALTER TABLE office_queue ADD COLUMN recipient_bot_id TEXT;

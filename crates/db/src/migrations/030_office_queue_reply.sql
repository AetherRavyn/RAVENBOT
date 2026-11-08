-- A queued office message keeps the message it quotes. The office thread may
-- be idle when the item is enqueued, so the reply target is stored with the
-- queue item and attached when the turn actually starts.
ALTER TABLE office_queue ADD COLUMN reply_to_id TEXT;

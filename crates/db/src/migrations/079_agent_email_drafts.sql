-- Agent email drafts.
--
-- A draft is the only thing an agent may create. Sending is a separate,
-- owner-only command a person invokes after reviewing the row here. Bodies and
-- recipient addresses stay in this table; audit rows carry only the draft id
-- and recipient count.
CREATE TABLE IF NOT EXISTS email_drafts (
  id TEXT PRIMARY KEY NOT NULL,
  bot_id TEXT NOT NULL,
  thread_id TEXT,
  run_id TEXT,
  to_addresses TEXT NOT NULL,
  cc_addresses TEXT NOT NULL,
  subject TEXT NOT NULL,
  body TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'draft',
  error TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  sent_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_email_drafts_created_at
    ON email_drafts(created_at DESC);

CREATE INDEX IF NOT EXISTS idx_email_drafts_status
    ON email_drafts(status, created_at DESC);

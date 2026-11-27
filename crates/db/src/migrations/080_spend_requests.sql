-- Spend requests and owner decisions.
--
-- Commerce v1 does not move money. The agent may only insert a `pending` row;
-- an owner approves or denies it, opens the merchant checkout in a browser, and
-- marks the result. Amounts are minor units in one workspace currency. No card
-- data is stored here.
CREATE TABLE IF NOT EXISTS spend_requests (
  id TEXT PRIMARY KEY NOT NULL,
  bot_id TEXT NOT NULL,
  thread_id TEXT,
  run_id TEXT,
  title TEXT NOT NULL,
  merchant TEXT NOT NULL,
  merchant_url TEXT NOT NULL,
  amount_minor INTEGER NOT NULL,
  currency TEXT NOT NULL,
  reason TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'pending', -- pending|approved|denied|purchased|cancelled
  decided_by TEXT,
  decided_at TEXT,
  decision_note TEXT,
  purchased_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_spend_requests_created_at
    ON spend_requests(created_at DESC);

CREATE INDEX IF NOT EXISTS idx_spend_requests_status
    ON spend_requests(status, created_at DESC);

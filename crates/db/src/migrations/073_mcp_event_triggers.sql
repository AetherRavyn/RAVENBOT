-- Event triggers fed by a long-lived MCP notification listener.
--
-- `event` is a JSON-RPC notification method (`notifications/message`,
-- `notifications/resources/updated`, or `*`). `resource_uri` is required only
-- when the row needs a `resources/subscribe` call before the server will emit
-- updates. `match_text` is an optional case-insensitive substring filter over
-- the method, resource URI and params.
CREATE TABLE IF NOT EXISTS mcp_event_triggers (
  id TEXT PRIMARY KEY,
  routine_id TEXT NOT NULL,
  server_id TEXT NOT NULL,
  event TEXT NOT NULL,
  resource_uri TEXT NOT NULL DEFAULT '',
  match_text TEXT NOT NULL DEFAULT '',
  cooldown_secs INTEGER NOT NULL DEFAULT 60,
  enabled INTEGER NOT NULL DEFAULT 1,
  last_fired_at TEXT,
  last_error TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE (routine_id, server_id, event, resource_uri, match_text)
);

CREATE INDEX IF NOT EXISTS idx_mcp_event_triggers_server
  ON mcp_event_triggers(server_id, enabled);
CREATE INDEX IF NOT EXISTS idx_mcp_event_triggers_routine
  ON mcp_event_triggers(routine_id);

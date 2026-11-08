-- Retire placeholder plugin rows created by the old catalog sync.
--
-- Those rows were named like actions (`gmail_create_draft`) but carried a
-- 58-byte stub instead of an OpenAPI document, so they had zero executable
-- operations. Keeping them made the UI claim tools that could never run.
ALTER TABLE plugins ADD COLUMN requires_auth INTEGER NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS plugin_credentials (
    plugin_id TEXT PRIMARY KEY REFERENCES plugins(id) ON DELETE CASCADE,
    header_name TEXT NOT NULL DEFAULT 'Authorization',
    header_value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

DELETE FROM bot_plugins
WHERE plugin_id IN (
    SELECT id FROM plugins
    WHERE COALESCE(length(openapi_spec), 0) < 200
      AND id NOT IN (SELECT plugin_id FROM openapi_operations)
);

DELETE FROM plugins
WHERE COALESCE(length(openapi_spec), 0) < 200
  AND id NOT IN (SELECT plugin_id FROM openapi_operations);

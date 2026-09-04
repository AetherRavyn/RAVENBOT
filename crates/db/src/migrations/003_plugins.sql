-- 1000+ plugins (Composio + OpenAPI) — native skills
CREATE TABLE IF NOT EXISTS plugins (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    logo TEXT,
    manifest_url TEXT,
    openapi_spec TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS bot_plugins (
    bot_id TEXT NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    plugin_id TEXT NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    enabled INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (bot_id, plugin_id)
);
CREATE INDEX IF NOT EXISTS idx_bot_plugins_bot ON bot_plugins(bot_id);
CREATE INDEX IF NOT EXISTS idx_bot_plugins_plugin ON bot_plugins(plugin_id);

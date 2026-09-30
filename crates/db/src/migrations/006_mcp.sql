-- MCP servers — 60+ as native tools
CREATE TABLE IF NOT EXISTS mcp_servers (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    category TEXT NOT NULL,
    icon TEXT,
    command TEXT NOT NULL,
    args TEXT NOT NULL DEFAULT '[]',
    env_keys TEXT NOT NULL DEFAULT '[]',
    enabled INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS mcp_bot_servers (
    bot_id TEXT NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    server_id TEXT NOT NULL REFERENCES mcp_servers(id) ON DELETE CASCADE,
    enabled INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (bot_id, server_id)
);
CREATE INDEX IF NOT EXISTS idx_mcp_bot_servers_bot ON mcp_bot_servers(bot_id);

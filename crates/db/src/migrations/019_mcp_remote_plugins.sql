-- P8 batch 3: remote (HTTP) MCP servers, OpenAPI server_base persistence,
-- global plugin scope
-- Version 19
-- NOTE: the migration runner splits statements on semicolons — every
-- statement here is terminated and no comment or literal contains one.

ALTER TABLE mcp_servers ADD COLUMN url TEXT;
ALTER TABLE mcp_servers ADD COLUMN transport TEXT NOT NULL DEFAULT 'stdio';
ALTER TABLE mcp_servers ADD COLUMN headers_json TEXT;
ALTER TABLE openapi_operations ADD COLUMN server_base TEXT NOT NULL DEFAULT '';
ALTER TABLE plugins ADD COLUMN enabled_global INTEGER NOT NULL DEFAULT 0;

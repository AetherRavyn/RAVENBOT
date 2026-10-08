-- Migration 018: User-defined model providers and per-provider base URL overrides
-- custom_providers: any OpenAI-compatible / Anthropic / Ollama endpoint the user adds.
CREATE TABLE IF NOT EXISTS custom_providers (id TEXT PRIMARY KEY NOT NULL, display_name TEXT NOT NULL, kind TEXT NOT NULL DEFAULT 'openai', base_url TEXT NOT NULL, default_model TEXT NOT NULL DEFAULT '', supports_tools INTEGER NOT NULL DEFAULT 1, enabled INTEGER NOT NULL DEFAULT 1, created_at TEXT NOT NULL DEFAULT (datetime('now')), updated_at TEXT NOT NULL DEFAULT (datetime('now')));
-- provider_base_urls: opt-in base URL override for built-in providers (absence = compiled default)
CREATE TABLE IF NOT EXISTS provider_base_urls (provider TEXT PRIMARY KEY NOT NULL, base_url TEXT NOT NULL, updated_at TEXT NOT NULL DEFAULT (datetime('now')));

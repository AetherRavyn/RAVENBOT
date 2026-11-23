-- Persist the live context-meter buckets so a reload keeps the breakdown, not
-- only the total.
ALTER TABLE messages ADD COLUMN context_system_tokens INTEGER;
ALTER TABLE messages ADD COLUMN context_tool_tokens INTEGER;
ALTER TABLE messages ADD COLUMN context_memory_tokens INTEGER;
ALTER TABLE messages ADD COLUMN context_message_tokens INTEGER;

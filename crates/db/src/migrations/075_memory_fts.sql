-- Full-text indexes for agent and office memory.
--
-- Retrieval used to scan every stored vector for a bot and rely on a 128-dim
-- hash vector alone. That made common-word queries noisy and made the cost grow
-- with the whole memory table. A standalone FTS5 table gives the retriever a
-- lexical candidate set (BM25-ranked, Porter-stemmed) that vectors can then
-- rerank. It also means a memory without an embedding is still findable.
--
-- Standalone tables, like messages_fts, keep the owning ids alongside the
-- searchable text. Triggers keep them in sync and the backfill is idempotent.
CREATE VIRTUAL TABLE IF NOT EXISTS memory_facts_fts USING fts5(
    fact_id UNINDEXED,
    bot_id UNINDEXED,
    content,
    tokenize = 'porter unicode61'
);

CREATE TRIGGER IF NOT EXISTS memory_facts_fts_after_insert
AFTER INSERT ON memory_facts
BEGIN
    INSERT INTO memory_facts_fts(fact_id, bot_id, content)
    VALUES (new.id, new.bot_id, new.content);
END;

CREATE TRIGGER IF NOT EXISTS memory_facts_fts_after_delete
AFTER DELETE ON memory_facts
BEGIN
    DELETE FROM memory_facts_fts WHERE fact_id = old.id;
END;

CREATE TRIGGER IF NOT EXISTS memory_facts_fts_after_update
AFTER UPDATE OF content, bot_id ON memory_facts
BEGIN
    DELETE FROM memory_facts_fts WHERE fact_id = old.id;
    INSERT INTO memory_facts_fts(fact_id, bot_id, content)
    VALUES (new.id, new.bot_id, new.content);
END;

INSERT INTO memory_facts_fts(fact_id, bot_id, content)
SELECT m.id, m.bot_id, m.content
FROM memory_facts m
WHERE NOT EXISTS (
    SELECT 1 FROM memory_facts_fts f WHERE f.fact_id = m.id
);

CREATE VIRTUAL TABLE IF NOT EXISTS office_memories_fts USING fts5(
    memory_id UNINDEXED,
    chatroom_id UNINDEXED,
    content,
    tokenize = 'porter unicode61'
);

CREATE TRIGGER IF NOT EXISTS office_memories_fts_after_insert
AFTER INSERT ON office_memories
BEGIN
    INSERT INTO office_memories_fts(memory_id, chatroom_id, content)
    VALUES (new.id, new.chatroom_id, new.content);
END;

CREATE TRIGGER IF NOT EXISTS office_memories_fts_after_delete
AFTER DELETE ON office_memories
BEGIN
    DELETE FROM office_memories_fts WHERE memory_id = old.id;
END;

CREATE TRIGGER IF NOT EXISTS office_memories_fts_after_update
AFTER UPDATE OF content, chatroom_id ON office_memories
BEGIN
    DELETE FROM office_memories_fts WHERE memory_id = old.id;
    INSERT INTO office_memories_fts(memory_id, chatroom_id, content)
    VALUES (new.id, new.chatroom_id, new.content);
END;

INSERT INTO office_memories_fts(memory_id, chatroom_id, content)
SELECT m.id, m.chatroom_id, m.content
FROM office_memories m
WHERE NOT EXISTS (
    SELECT 1 FROM office_memories_fts f WHERE f.memory_id = m.id
);

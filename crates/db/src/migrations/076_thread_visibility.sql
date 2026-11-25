-- Not every thread is a conversation the user should see in an agent's history.
-- Office node runs, delegations, routines and channel tasks each get a thread so
-- their reasoning/tools stay attributable, but listing all of them in the chat
-- switcher turns one assistant into a pile of internal work logs.
--
-- Existing rows are classified by the title conventions the runtime already
-- used. New writes set the fields explicitly through Thread::new_internal.
ALTER TABLE threads ADD COLUMN kind TEXT NOT NULL DEFAULT 'chat';
ALTER TABLE threads ADD COLUMN visible INTEGER NOT NULL DEFAULT 1;

UPDATE threads SET kind = 'office', visible = 0
WHERE id IN (SELECT thread_id FROM chatroom_threads);

UPDATE threads SET kind = 'delegation', visible = 0
WHERE title LIKE 'Delegation:%';

UPDATE threads SET kind = 'routine', visible = 0
WHERE title LIKE 'Routine:%';

UPDATE threads SET kind = 'node', visible = 0
WHERE title LIKE 'Channel task:%' OR title LIKE 'Node:%';

CREATE INDEX IF NOT EXISTS idx_threads_visible
    ON threads(bot_id, visible, updated_at DESC);

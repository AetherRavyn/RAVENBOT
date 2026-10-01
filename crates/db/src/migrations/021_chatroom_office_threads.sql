-- An office needs to relate to *many* threads: its own conversation, plus one
-- thread per agent it delegates to. `chatroom_threads` cannot express that —
-- `chatroom_id` is its primary key, so the table asserts one thread per office —
-- and the consequence was silent and total. Linking a delegated agent's thread
-- with `INSERT OR REPLACE` deleted the office's own row and inserted the child's.
--
-- After a single delegation every office that had ever delegated opened onto its
-- child's transcript, and its real conversation sat in `messages` under a thread
-- id nothing pointed at any more. Nothing errored, nothing warned, and no rows
-- were deleted, so it presented as "the office's history disappeared".
--
-- The obvious repair is to rebuild `chatroom_threads` as a composite key plus a
-- `kind` column. That needs a `CREATE`/`INSERT SELECT`/`DROP`/`RENAME` dance,
-- and the `RENAME` reliably fails under this project's pooled connections --
-- `DROP TABLE` is not visible to the connection that runs the next statement, so
-- the new name still looks occupied. Every statement is wrapped in its own
-- implicit transaction because the runner has no transaction support, so there is
-- no atomicity either. A migration that can leave the link table half-rebuilt on
-- an existing user's database is worse than the bug it fixes.
--
-- So this is additive: `chatroom_threads` keeps its exact shape and its exact
-- meaning -- the office's own conversation -- and the one-to-many case gets its
-- own table. Nothing existing is altered, so an old database and a freshly
-- migrated one behave identically, which is the property a migration most needs
-- to have. The cost is one extra lookup on the reverse path, which is indexed.
--
-- The indexes matter on their own account: the reverse lookup ("which office is
-- this thread working in?") runs on every tool call, to enforce capabilities and
-- to resolve the workspace, and it was a full table scan.
CREATE TABLE IF NOT EXISTS chatroom_office_threads (
    chatroom_id TEXT NOT NULL REFERENCES chatrooms(id) ON DELETE CASCADE,
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (chatroom_id, thread_id)
);

CREATE INDEX IF NOT EXISTS idx_chatroom_threads_thread
    ON chatroom_threads(thread_id);

CREATE INDEX IF NOT EXISTS idx_chatroom_office_threads_thread
    ON chatroom_office_threads(thread_id);

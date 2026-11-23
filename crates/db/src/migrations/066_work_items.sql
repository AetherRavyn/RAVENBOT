-- Durable work ledger.
--
-- The office TaskGraph is the scheduler; this is the record. A graph snapshot
-- answers "what should run next?" after a restart. A work item answers "what
-- happened, who owns it, what did it produce, and why is it blocked?" for a
-- human or another agent, long after the graph is gone.
--
-- `work_items.id` is the graph node id for office tasks, so the ledger and the
-- scheduler can be reconciled without a second mapping table.
CREATE TABLE IF NOT EXISTS work_items (
    id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL DEFAULT 'office_task',
    parent_id TEXT,
    office_id TEXT,
    thread_id TEXT,
    run_id TEXT,
    title TEXT NOT NULL,
    instruction TEXT NOT NULL DEFAULT '',
    contract_json TEXT NOT NULL DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'pending',
    owner_bot_id TEXT,
    reviewer_bot_id TEXT,
    attempts INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 1,
    priority INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f+00:00', 'now')),
    started_at TEXT,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f+00:00', 'now')),
    completed_at TEXT,
    lease_owner TEXT,
    lease_expires_at TEXT,
    heartbeat_at TEXT,
    workspace TEXT,
    result TEXT,
    error TEXT,
    metadata_json TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS work_links (
    parent_id TEXT NOT NULL,
    child_id TEXT NOT NULL,
    PRIMARY KEY (parent_id, child_id),
    FOREIGN KEY (parent_id) REFERENCES work_items(id) ON DELETE CASCADE,
    FOREIGN KEY (child_id) REFERENCES work_items(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS work_comments (
    id TEXT PRIMARY KEY NOT NULL,
    work_item_id TEXT NOT NULL,
    author TEXT NOT NULL,
    body TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f+00:00', 'now')),
    FOREIGN KEY (work_item_id) REFERENCES work_items(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS work_artifacts (
    id TEXT PRIMARY KEY NOT NULL,
    work_item_id TEXT NOT NULL,
    path TEXT NOT NULL,
    sha256 TEXT,
    mime TEXT,
    size INTEGER,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f+00:00', 'now')),
    FOREIGN KEY (work_item_id) REFERENCES work_items(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_work_items_office
    ON work_items(office_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_work_items_thread
    ON work_items(thread_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_work_items_status
    ON work_items(status, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_work_comments_item
    ON work_comments(work_item_id, created_at ASC);
CREATE INDEX IF NOT EXISTS idx_work_artifacts_item
    ON work_artifacts(work_item_id, created_at ASC);
CREATE INDEX IF NOT EXISTS idx_work_links_parent
    ON work_links(parent_id);

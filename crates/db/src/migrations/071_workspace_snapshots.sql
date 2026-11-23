-- Before-images for the files an agent changed during one run.
--
-- The snapshot row is created lazily, on the first file-writing tool call, so
-- a read-only run pays nothing. The blob column is a SHA-256 key into
-- <RAVENBOT_HOME>/snapshots/blobs; NULL with existed = 1 means the file was
-- there but could not be snapshotted (too large or unreadable), and existed = 0
-- means the run created the path. Keeping "absent" distinct from "unavailable"
-- is what lets revert delete a created file instead of writing an empty one.
CREATE TABLE IF NOT EXISTS workspace_snapshots (
    id TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL,
    thread_id TEXT,
    root TEXT NOT NULL,
    kind TEXT NOT NULL DEFAULT 'files',
    complete INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_workspace_snapshots_run
    ON workspace_snapshots(run_id);

CREATE TABLE IF NOT EXISTS workspace_snapshot_entries (
    snapshot_id TEXT NOT NULL,
    path TEXT NOT NULL,
    root TEXT NOT NULL DEFAULT '',
    blob TEXT,
    existed INTEGER NOT NULL DEFAULT 1,
    sha256 TEXT,
    mode INTEGER,
    size INTEGER NOT NULL DEFAULT 0,
    note TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    PRIMARY KEY (snapshot_id, path),
    FOREIGN KEY (snapshot_id) REFERENCES workspace_snapshots(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_workspace_snapshot_entries_snapshot
    ON workspace_snapshot_entries(snapshot_id, path);

-- The before/after hashes let a revert distinguish "this is still the file the
-- run wrote" from "someone changed it after the run". The absolute snapshot
-- path is the join key; `path` stays exactly what the tool saw.
ALTER TABLE file_changes ADD COLUMN snapshot_path TEXT;
ALTER TABLE file_changes ADD COLUMN before_sha256 TEXT;
ALTER TABLE file_changes ADD COLUMN after_sha256 TEXT;

CREATE INDEX IF NOT EXISTS idx_file_changes_snapshot_path
    ON file_changes(snapshot_path, created_at);

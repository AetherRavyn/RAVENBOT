-- Audit viewer: an indexable event kind and canonical UTC timestamps.
--
-- The event JSON remains the record of truth. `event_kind` is the stable
-- filter key for the global audit view, backfilled from the externally-tagged
-- serde name so a database created before this migration is searchable.
ALTER TABLE audit_log ADD COLUMN event_kind TEXT NOT NULL DEFAULT '';

UPDATE audit_log SET event_kind = COALESCE(CASE
    WHEN json_valid(event) = 0 THEN ''
    WHEN json_type(event) = 'object' THEN (SELECT key FROM json_each(audit_log.event) LIMIT 1)
    ELSE json_extract(event, '$')
END, '');

-- Logged entries already use RFC 3339, but older defaults used SQLite's
-- space-separated form. Normalise every parseable instant to UTC with exactly
-- three fractional digits, so lexicographic ordering and range filters agree
-- with the real instant.
UPDATE audit_log
SET timestamp = strftime('%Y-%m-%dT%H:%M:%f+00:00', timestamp)
WHERE strftime('%Y-%m-%dT%H:%M:%f+00:00', timestamp) IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_audit_log_kind_timestamp
    ON audit_log(event_kind, timestamp DESC);

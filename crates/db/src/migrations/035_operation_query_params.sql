-- Query parameter names for each operation. A merged tool schema contains both
-- query and body fields. Execution needs to know which fields belong in the URL
-- so a POST does not send `dry_run=true` inside its JSON body.
ALTER TABLE openapi_operations ADD COLUMN query_params TEXT NOT NULL DEFAULT '[]';

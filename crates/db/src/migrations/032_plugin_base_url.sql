-- Optional server override for specs whose servers[0].url is a placeholder.
-- Jira Cloud, for example, ships https://your-domain.atlassian.net. A real
-- installation points at its own tenant without rewriting the stored spec.
ALTER TABLE plugins ADD COLUMN base_url_override TEXT;

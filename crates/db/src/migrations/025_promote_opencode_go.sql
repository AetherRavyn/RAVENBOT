-- OpenCode Go is a first-class provider now (`ModelProvider::OpenCodeGo`).
--
-- Before that, it could only be configured as a custom provider, and a row
-- with the official endpoint still exists in some databases. ProviderManager
-- ignores it for this id so the built-in always wins, but leaving it in place
-- means Settings shows two "OpenCode Go" entries and the custom one carries a
-- hardcoded `x-opencode-session` header that defeats the built-in's per-process
-- session id.
--
-- Only the official endpoint is removed. A custom gateway or proxy the user
-- actually points somewhere else is left untouched.
DELETE FROM custom_providers
WHERE id = 'opencode-go'
  AND (base_url LIKE '%opencode.ai/inference/go%' OR TRIM(base_url) = '');

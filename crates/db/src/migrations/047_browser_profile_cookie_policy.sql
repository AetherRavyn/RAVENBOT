-- Named browser profiles may inherit the global automatic cookie-refusal
-- setting or override it. Automatic per-agent/office profiles use the global
-- setting.
ALTER TABLE browser_profiles ADD COLUMN cookie_policy TEXT NOT NULL DEFAULT 'inherit';

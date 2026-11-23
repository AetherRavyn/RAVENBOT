-- Structured human-in-the-loop cards. Existing rows stay free-text questions;
-- `payload` is validated JSON and is only read for a non-question `kind`.
ALTER TABLE questions ADD COLUMN kind TEXT NOT NULL DEFAULT 'question';
ALTER TABLE questions ADD COLUMN payload TEXT;

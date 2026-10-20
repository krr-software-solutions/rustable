-- Soft-delete: archived habits disappear from the today view but keep history.
ALTER TABLE habits ADD COLUMN archived_at TEXT;

CREATE INDEX idx_habits_active ON habits (name) WHERE archived_at IS NULL;

PRAGMA user_version = 3;

-- "check-in" -> "log" across the product. RENAME TO also rewrites the
-- view and trigger that reference the old table name.
ALTER TABLE check_ins RENAME TO habit_logs;
ALTER TABLE habit_logs RENAME COLUMN day TO logged_on;

CREATE INDEX idx_habit_logs_logged_on ON habit_logs (logged_on DESC);

PRAGMA user_version = 7;

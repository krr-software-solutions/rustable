DROP INDEX IF EXISTS idx_habit_logs_logged_on;
ALTER TABLE habit_logs RENAME COLUMN logged_on TO day;
ALTER TABLE habit_logs RENAME TO check_ins;

PRAGMA user_version = 6;

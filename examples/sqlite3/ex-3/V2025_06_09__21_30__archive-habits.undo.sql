-- Must drop the partial index first: DROP COLUMN refuses while an index uses the column.
DROP INDEX IF EXISTS idx_habits_active;
ALTER TABLE habits DROP COLUMN archived_at;

PRAGMA user_version = 2;

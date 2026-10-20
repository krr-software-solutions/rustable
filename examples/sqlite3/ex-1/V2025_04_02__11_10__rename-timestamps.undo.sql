ALTER TABLE notebooks RENAME COLUMN created_at TO created;
ALTER TABLE notes RENAME COLUMN updated_at TO updated;
ALTER TABLE notes RENAME COLUMN created_at TO created;

PRAGMA user_version = 3;

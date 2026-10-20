-- Align naming with the sync server: created/updated -> created_at/updated_at.
-- RENAME COLUMN also rewrites references inside indexes and triggers.
ALTER TABLE notes RENAME COLUMN created TO created_at;
ALTER TABLE notes RENAME COLUMN updated TO updated_at;
ALTER TABLE notebooks RENAME COLUMN created TO created_at;

PRAGMA user_version = 4;

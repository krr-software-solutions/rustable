COMMENT ON COLUMN bookmarks.label IS NULL;
ALTER TABLE bookmarks RENAME COLUMN label TO title;

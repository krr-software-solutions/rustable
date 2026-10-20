-- Reverse the rebuild back to the v1 shape.
PRAGMA foreign_keys = OFF;

BEGIN TRANSACTION;

CREATE TABLE bookmarks_old (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    url          TEXT    NOT NULL,
    title        TEXT,
    favicon_blob BLOB,
    read_later   INTEGER NOT NULL DEFAULT 0,
    added_at     TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;

INSERT INTO bookmarks_old (id, url, title, read_later, added_at)
SELECT id, url, title, status = 'unread', added_at
  FROM bookmarks;

DROP TABLE bookmarks;
ALTER TABLE bookmarks_old RENAME TO bookmarks;

CREATE INDEX idx_bookmarks_read_later ON bookmarks (added_at) WHERE read_later = 1;

COMMIT;

PRAGMA foreign_keys = ON;
PRAGMA user_version = 1;

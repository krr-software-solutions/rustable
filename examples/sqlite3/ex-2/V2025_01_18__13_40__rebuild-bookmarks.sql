-- Rebuild `bookmarks` (SQLite "12-step" ALTER) because ALTER TABLE cannot:
--   * add a CHECK constraint to an existing column (url must be http/https)
--   * turn read_later (0/1 flag) into a CHECK-constrained status enum
--   * drop favicon_blob while keeping STRICT (favicons now live in a cache dir)
-- foreign_keys must be toggled OUTSIDE the transaction (it is a no-op inside one).
PRAGMA foreign_keys = OFF;

BEGIN TRANSACTION;

CREATE TABLE bookmarks_new (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    url      TEXT    NOT NULL CHECK (url LIKE 'http://%' OR url LIKE 'https://%'),
    title    TEXT    NOT NULL DEFAULT '',
    status   TEXT    NOT NULL DEFAULT 'saved' CHECK (status IN ('saved', 'unread', 'archived')),
    added_at TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;

INSERT INTO bookmarks_new (id, url, title, status, added_at)
SELECT id,
       url,
       coalesce(title, ''),
       CASE read_later WHEN 1 THEN 'unread' ELSE 'saved' END,
       added_at
  FROM bookmarks;

DROP TABLE bookmarks;
ALTER TABLE bookmarks_new RENAME TO bookmarks;

CREATE UNIQUE INDEX idx_bookmarks_url ON bookmarks (lower(url));
CREATE INDEX idx_bookmarks_unread ON bookmarks (added_at) WHERE status = 'unread';

PRAGMA foreign_key_check;

COMMIT;

PRAGMA foreign_keys = ON;
PRAGMA user_version = 2;

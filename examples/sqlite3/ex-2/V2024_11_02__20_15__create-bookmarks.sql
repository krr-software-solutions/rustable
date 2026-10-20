-- "Stash": a local-first bookmark manager.
PRAGMA foreign_keys = ON;

CREATE TABLE bookmarks (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    url          TEXT    NOT NULL,
    title        TEXT,
    favicon_blob BLOB,
    read_later   INTEGER NOT NULL DEFAULT 0,
    added_at     TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;

-- Case-insensitive de-duplication of URLs via an expression index.
CREATE UNIQUE INDEX idx_bookmarks_url ON bookmarks (lower(url));
CREATE INDEX idx_bookmarks_read_later ON bookmarks (added_at) WHERE read_later = 1;

-- Tags are tiny lookup rows keyed by name: no need for a rowid.
CREATE TABLE tags (
    name TEXT NOT NULL PRIMARY KEY COLLATE NOCASE
) STRICT, WITHOUT ROWID;

CREATE TABLE bookmark_tags (
    bookmark_id INTEGER NOT NULL REFERENCES bookmarks (id) ON DELETE CASCADE,
    tag         TEXT    NOT NULL REFERENCES tags (name) ON DELETE CASCADE ON UPDATE CASCADE,
    PRIMARY KEY (bookmark_id, tag)
) STRICT, WITHOUT ROWID;

CREATE INDEX idx_bookmark_tags_tag ON bookmark_tags (tag);

INSERT INTO tags (name) VALUES ('rust'), ('sqlite'), ('to-read');
INSERT INTO bookmarks (url, title, read_later) VALUES
    ('https://sqlite.org/lang_altertable.html', 'ALTER TABLE', 1),
    ('http://example.com', 'Example', 0);
INSERT INTO bookmark_tags (bookmark_id, tag) VALUES (1, 'sqlite'), (1, 'to-read');

PRAGMA user_version = 1;

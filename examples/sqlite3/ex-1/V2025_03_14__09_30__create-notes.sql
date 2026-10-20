-- Initial schema for the "Jot" local-first notes app.
PRAGMA foreign_keys = ON;

CREATE TABLE notebooks (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT    NOT NULL COLLATE NOCASE,
    color      TEXT    CHECK (color IS NULL OR color GLOB '#[0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F]'),
    created    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (name)
);

CREATE TABLE notes (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    notebook_id INTEGER NOT NULL REFERENCES notebooks (id) ON DELETE CASCADE,
    title       TEXT    NOT NULL DEFAULT '' CHECK (length(title) <= 200),
    body        TEXT    NOT NULL DEFAULT '',
    created     TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated     TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX idx_notes_notebook ON notes (notebook_id, updated DESC);

INSERT INTO notebooks (name, color) VALUES ('Inbox', '#3366ff');

PRAGMA user_version = 1;

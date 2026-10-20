-- "Streaky": an offline habit tracker.
PRAGMA foreign_keys = ON;

CREATE TABLE habits (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT    NOT NULL COLLATE NOCASE CHECK (length(trim(name)) > 0),
    frequency  TEXT    NOT NULL DEFAULT 'daily' CHECK (frequency IN ('daily', 'weekly')),
    created_at TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE UNIQUE INDEX idx_habits_name ON habits (name);

PRAGMA user_version = 1;

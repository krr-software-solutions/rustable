-- Full-text search over notes, backed by an external-content FTS5 table.
CREATE VIRTUAL TABLE notes_fts USING fts5(
    title,
    body,
    content = 'notes',
    content_rowid = 'id',
    tokenize = 'unicode61 remove_diacritics 2'
);

-- Keep the index in sync. Note the semicolons inside the trigger bodies.
CREATE TRIGGER notes_fts_ai AFTER INSERT ON notes BEGIN
    INSERT INTO notes_fts (rowid, title, body) VALUES (new.id, new.title, new.body);
END;

CREATE TRIGGER notes_fts_ad AFTER DELETE ON notes BEGIN
    INSERT INTO notes_fts (notes_fts, rowid, title, body) VALUES ('delete', old.id, old.title, old.body);
END;

CREATE TRIGGER notes_fts_au AFTER UPDATE OF title, body ON notes BEGIN
    INSERT INTO notes_fts (notes_fts, rowid, title, body) VALUES ('delete', old.id, old.title, old.body);
    INSERT INTO notes_fts (rowid, title, body) VALUES (new.id, new.title, new.body);
END;

-- Backfill existing notes.
INSERT INTO notes_fts (notes_fts) VALUES ('rebuild');

PRAGMA user_version = 3;

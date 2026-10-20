-- Users can pin notes to the top of a notebook.
ALTER TABLE notes ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0 CHECK (pinned IN (0, 1));

-- Only a handful of notes are ever pinned, so a partial index stays tiny.
CREATE INDEX idx_notes_pinned
    ON notes (notebook_id, updated DESC)
    WHERE pinned = 1;

PRAGMA user_version = 2;

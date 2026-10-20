# ex-1: "Jot" (local-first notes app)

A single-user notes app storing everything in one SQLite file. It starts with
notebooks + notes, adds pinning, bolts on FTS5 full-text search, renames its
timestamp columns to match the sync server, and finally drops the old notebook
color in favour of an emoji icon (a data migration followed by a breaking
`DROP COLUMN`). Every migration bumps `PRAGMA user_version`.

| # | Migration | What it does | Undo file | Reversible | Backward compatible | Forward compatible |
|---|-----------|--------------|-----------|------------|---------------------|--------------------|
| 1 | `V2025_03_14__09_30__create-notes` | `PRAGMA foreign_keys = ON`; creates `notebooks` / `notes` (AUTOINCREMENT, `COLLATE NOCASE`, `CHECK`, `strftime` defaults, FK `ON DELETE CASCADE`), an index, seeds an "Inbox" notebook | yes | lossy (structure restorable, data lost) | yes (no prior app) | no |
| 2 | `V2025_03_14__10_05__add-note-pinned` | `ADD COLUMN pinned … DEFAULT 0 CHECK`, partial index `WHERE pinned = 1` | no | lossy (drop index + drop column loses pin flags) | yes (column has a default) | no (new code reads `pinned`) |
| 3 | `V2025_03_20__16_45__notes-fts` | FTS5 external-content virtual table, three sync triggers with multi-statement `BEGIN … ; … ; END` bodies, `'rebuild'` backfill | yes | lossless (index is fully derived from `notes`) | yes (triggers keep it in sync transparently) | no (new code queries `notes_fts`) |
| 4 | `V2025_04_02__11_10__rename-timestamps` | `RENAME COLUMN` created/updated -> created_at/updated_at (SQLite rewrites the index too) | yes | lossless | **no** (old code uses `created`/`updated`) | **no** |
| 5 | `V2025_04_15__08_00__drop-notebook-color` | `ADD COLUMN icon`, `UPDATE` mapping color -> emoji, `DROP COLUMN color` | no | irreversible (colors gone; mapping is many-to-one) | **no** (old code reads/writes `color`) | no (new code reads `icon`) |

Notes for the tool:
- Migration 1 contains `PRAGMA foreign_keys = ON` — `sqlparser` 0.63 fails to parse `PRAGMA x = ON`.
- The pragma is per-connection: it does not persist in the file, so the tool/app must set it on every connection.
- Migration 3 has semicolons inside trigger bodies; a naive split on `;` breaks it.
- `CREATE VIRTUAL TABLE … USING fts5(...)` takes module arguments (`content = 'notes'`, `tokenize = '…'`) that aren't ordinary column definitions.
- Migration 5 contains non-ASCII (emoji) string literals.

# ex-2: "Stash" (local-first bookmark manager)

A two-migration history. Version 1 creates `STRICT` tables, `WITHOUT ROWID`
tag tables and a unique **expression** index on `lower(url)`. Version 2 needs
changes `ALTER TABLE` can't make (add a `CHECK` to an existing column, replace a
0/1 flag with a constrained enum, drop a column while rebuilding anyway), so it
does the classic SQLite **12-step table rebuild**: create `bookmarks_new`, copy
with `INSERT … SELECT`, drop the old table, rename the new one, recreate indexes,
run `PRAGMA foreign_key_check`, all inside an explicit `BEGIN … COMMIT`, with
`PRAGMA foreign_keys` toggled *outside* the transaction.

| # | Migration | What it does | Undo file | Reversible | Backward compatible | Forward compatible |
|---|-----------|--------------|-----------|------------|---------------------|--------------------|
| 1 | `V2024_11_02__20_15__create-bookmarks` | `STRICT` `bookmarks` (AUTOINCREMENT, `CURRENT_TIMESTAMP`), unique expression index, partial index, `STRICT, WITHOUT ROWID` `tags` (`COLLATE NOCASE` PK) and `bookmark_tags` (FKs with `ON DELETE/UPDATE CASCADE`), seed rows | yes | lossy (structure restorable, data lost) | yes (no prior app) | no |
| 2 | `V2025_01_18__13_40__rebuild-bookmarks` | 12-step rebuild: adds `CHECK (url LIKE 'http%…')`, `read_later` -> `status` enum (data migration), `title` NULL -> `''`, drops `favicon_blob` | yes (**intentionally wrong**) | lossy (`favicon_blob` data gone; `archived` collapses to `read_later = 0`; `''` titles stay `''`, not `NULL`) | **no** (`read_later`/`favicon_blob` gone; old inserts may violate the new `CHECK`) | **no** (`status` column doesn't exist yet) |

## The intentionally wrong undo

`V2025_01_18__13_40__rebuild-bookmarks.undo.sql` executes fine. But it **forgets to
recreate the unique expression index `idx_bookmarks_url`**. After undo the schema
looks like v1 but duplicate URLs (e.g. `HTTP://EXAMPLE.COM` vs `http://example.com`)
are now accepted. A schema-diff check of "after up+undo" against "before up" should
catch the missing index. It is also lossy in data terms (see table), which is
expected for this kind of change.

Notes for the tool:
- Migration 2 opens its own transaction (`BEGIN TRANSACTION; … COMMIT;`). If the
  tool wraps each migration in a transaction, it fails with "cannot start a
  transaction within a transaction". Also, `PRAGMA foreign_keys` is a no-op inside a
  transaction, so the tool can't just strip the BEGIN/COMMIT either.
- Both files use `PRAGMA foreign_keys = ON/OFF`, which `sqlparser` 0.63 can't parse.
- `PRAGMA foreign_key_check;` returns rows (one per violation) instead of
  failing, so a careful tool would treat a non-empty result as an error.
- `STRICT, WITHOUT ROWID` table options come after the closing paren.

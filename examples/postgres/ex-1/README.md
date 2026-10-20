# ex-1 — Pinboard (bookmarks app), 4 migrations

A small bookmarking service: users save URLs with tags, can soft-archive them, and
later the `title` column is renamed to `label`.

Features exercised: `bigserial`, `timestamptz DEFAULT now()`, unique expression index
(`lower(email)`), `text[]` + GIN index, FK `ON DELETE CASCADE`, regex `CHECK`,
`CREATE INDEX CONCURRENTLY` with a partial `WHERE`, `RENAME COLUMN`, `COMMENT ON`,
`--` and `/* */` comments.

| # | Migration | What it does | Undo file | Reversible | Backward compatible | Forward compatible |
|---|-----------|--------------|-----------|------------|---------------------|--------------------|
| 1 | `V2025_01_06__10_15__create-users` | Create `users` + unique index on `lower(email)` | yes | lossy (structure restorable, any user rows lost) | yes | no (new code needs the table) |
| 2 | `V2025_01_20__14_00__create-bookmarks` | Create `bookmarks` (FK cascade, `text[]` tags, GIN index) | no (auto: `DROP TABLE bookmarks`) | lossy (structure restorable, data lost) | yes | no |
| 3 | `V2025_02_03__09_45__add-bookmarks-archived-at` | Add nullable `archived_at`; `CREATE INDEX CONCURRENTLY` partial index | no (auto: drop index + drop column) | lossy (archive timestamps lost) | yes (nullable column) | no (new code reads `archived_at`) |
| 4 | `V2025_03_10__16_30__rename-bookmarks-title` | `RENAME COLUMN title TO label` + comment | yes | lossless | **no** (old code selects `title`) | **no** (new code selects `label`) |

Notes:
- Migration 3 uses `CREATE INDEX CONCURRENTLY`, which cannot run inside a transaction block —
  the tool must not wrap this file in `BEGIN/COMMIT`.
- A correct auto-undo for migration 3 must drop the index *before* (or implicitly with) the column;
  dropping the column alone happens to also drop the partial index because its predicate references it.

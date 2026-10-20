# ex-3: "Streaky" (offline habit tracker)

Eight migrations over a summer of development: habits, then check-ins and a
reminders table, soft-delete archiving, a stats view, triggers (auto-touch
`updated_at` and a `RAISE(ABORT)` guard), a data migration from a
`daily`/`weekly` enum to a numeric weekly target, a product-wide rename
(`check_ins` -> `habit_logs`, `day` -> `logged_on`, which SQLite propagates into
the view and trigger), and finally dropping the dead reminders table.

| # | Migration | What it does | Undo file | Reversible | Backward compatible | Forward compatible |
|---|-----------|--------------|-----------|------------|---------------------|--------------------|
| 1 | `V2025_06_01__07_00__create-habits` | `PRAGMA foreign_keys = ON`; `habits` (AUTOINCREMENT, `COLLATE NOCASE`, `CHECK`s, `strftime` default), unique index | yes | lossy (structure restorable, data lost) | yes (no prior app) | no |
| 2 | `V2025_06_01__07_20__create-check-ins` | `check_ins` (FK `ON DELETE CASCADE`, `CHECK (day = date(day))`, `UNIQUE`), `habit_reminders` `WITHOUT ROWID`, seed data | no | lossy (dropping tables loses data) | yes | no |
| 3 | `V2025_06_09__21_30__archive-habits` | `ADD COLUMN archived_at`, partial index `WHERE archived_at IS NULL` | yes | lossy (archive timestamps lost) | yes (nullable column) | no |
| 4 | `V2025_06_15__18_05__habit-stats-view` | `CREATE VIEW v_habit_stats` (LEFT JOIN + aggregates) | yes | lossless | yes | no (new code reads the view) |
| 5 | `V2025_07_02__08_45__habit-triggers` | `ADD COLUMN updated_at` + backfill; `AFTER UPDATE` trigger with `UPDATE …;` body; `BEFORE INSERT` trigger with `SELECT RAISE(ABORT, …);` | no | lossy (drop both triggers *before* dropping `updated_at`, or DROP COLUMN fails; values lost) | mostly: schema-compatible, but old code that checks in to archived habits now gets an error | no |
| 6 | `V2025_07_20__19_10__weekly-targets` | `ADD COLUMN target_per_week … CHECK`, `UPDATE` data migration from `frequency` | no | lossy (custom targets lost, only daily/weekly derivable) | yes (`frequency` kept), but habits inserted by old code get the default 7 | no |
| 7 | `V2025_08_04__12_00__rename-check-ins` | `RENAME TO habit_logs`, `RENAME COLUMN day TO logged_on`, new index | yes | lossless | **no** (old code uses `check_ins`/`day`) | **no** |
| 8 | `V2025_08_30__22_15__drop-reminders` | `DROP TABLE habit_reminders` | no | irreversible (table and data gone) | **no** (old code reads reminders) | **yes** (new code no longer touches the table) |

Notes for the tool:
- Migration 5 has two triggers with `BEGIN … ; END` bodies. One body contains
  `RAISE(ABORT, '…')`, which is SQLite-only syntax.
- Migration 7 shows that `ALTER TABLE … RENAME` rewrites dependent views and
  triggers (`v_habit_stats` and `check_ins_reject_archived` now reference
  `"habit_logs"` / `logged_on`). Undoing 7 renames them back.
- The undo for 3 only works directly after 3. Once 4 (the view) exists,
  `DROP COLUMN archived_at` fails. Undo ordering matters.
- `PRAGMA user_version` goes 1..8 and each undo resets it to the previous value.

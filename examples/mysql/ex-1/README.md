# ex-1 — tiny blog (3 migrations)

A minimal blog: `users` write `posts`. Later, users get a `display_name` that is
backfilled from their email address.

Exercises: `SET NAMES` version comments, `datetime(3)` with `ON UPDATE CURRENT_TIMESTAMP(3)`,
inline `UNIQUE KEY` / `KEY` / `CONSTRAINT … FOREIGN KEY`, enums, `CREATE TABLE IF NOT EXISTS`,
`ADD COLUMN … AFTER …, ALGORITHM=INSTANT`, a data backfill `UPDATE`, a prefix index
(`display_name(20)`) added with `ALGORITHM=INPLACE, LOCK=NONE`.

| Migration | What it does | Undo file | Reversible | Backward compatible | Forward compatible |
|---|---|---|---|---|---|
| `V2025_01_06__10_15__create-users` | Create `users` | yes | lossy (structure restorable, data lost) | yes | yes |
| `V2025_01_20__14_40__create-posts` | Create `posts` with FK to `users` | no (auto: `DROP TABLE posts`) | lossy (structure restorable, data lost) | yes | yes |
| `V2025_02_11__09_05__add-users-display-name` | Add nullable `users.display_name`, backfill from email, add prefix index | yes | lossy (backfilled values lost, but derivable) | yes (nullable column) | yes (new code must tolerate NULL; column not required yet) |

Validated against MySQL 8.4: all ups apply in order; every undo applies after its up and
restores the pre-migration schema byte-for-byte (per `mysqldump --no-data`).

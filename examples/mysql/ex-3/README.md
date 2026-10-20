# ex-3 — feature-flag service bootstrap (1 migration)

A single "big bang" migration bootstrapping a feature-flag service, in the style of a
hand-edited `mysqldump`: three tables (`environments`, `flags`, `flag_overrides`) plus seed data.

Exercises: `/*!40014 SET @OLD_FOREIGN_KEY_CHECKS=@@FOREIGN_KEY_CHECKS, FOREIGN_KEY_CHECKS=0 */`
and restore, tables created **child-first** (FKs reference tables that don't exist yet — only
valid because FK checks are off), a column named with a reserved word (`` `key` ``),
`decimal(5,2) unsigned`, `AUTO_INCREMENT=10` table option, `LOCK TABLES … WRITE` / `UNLOCK TABLES`,
`/*!40000 ALTER TABLE … DISABLE KEYS */`, multi-row `INSERT`, and a multi-table `DROP TABLE` in the undo.

| Migration | What it does | Undo file | Reversible | Backward compatible | Forward compatible |
|---|---|---|---|---|---|
| `V2025_06_30__22_10__bootstrap-feature-flags` | Create 3 tables (FK checks off, child-first), seed `environments` and `flags` | yes | lossy (structure restorable, data lost) | yes (no prior app) | **no** (the service can't run without its tables) |

Validated against MySQL 8.4: up applies, undo applies and restores an empty schema, up
re-applies cleanly after undo.

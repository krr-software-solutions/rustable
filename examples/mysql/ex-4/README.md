# ex-4 — helpdesk / ticketing (6 migrations)

A support desk inherited from an old PHP app (latin1 everywhere). Over a year it gets
converted to utf8mb4, ticket subjects are narrowed, `agents` is renamed to `staff_members`,
an auto-close stored procedure is added (and a dead tag table dropped), and requester emails
are normalized into their own table.

Exercises: latin1 tables with `COLLATE=latin1_swedish_ci`, **`ALTER TABLE … CONVERT TO CHARACTER SET`**,
table-option-only `ALTER TABLE … COMMENT=…, ROW_FORMAT=…`, type narrowing via `MODIFY COLUMN`
(`varchar(255)` → `varchar(64)`) with a preceding data fix-up, `RENAME TABLE`,
`ADD COLUMN … enum … AFTER …, ALGORITHM=INSTANT`, a stored procedure with
`DEFINER=CURRENT_USER`, characteristics (`MODIFIES SQL DATA`, `COMMENT`), `DECLARE`, wrapped in
`DELIMITER $$ … $$ DELIMITER ;`, `DROP TABLE`, `INSERT IGNORE … SELECT DISTINCT`,
multi-table `UPDATE … JOIN … SET`, and one `ALTER TABLE` mixing `DROP INDEX`, `DROP COLUMN`,
`ADD KEY`, `ADD CONSTRAINT … FOREIGN KEY`.

| Migration | What it does | Undo file | Reversible | Backward compatible | Forward compatible |
|---|---|---|---|---|---|
| `V2023_03_07__09_00__create-helpdesk-core` | Create `agents`, `tickets`, `ticket_legacy_tags` (latin1), seed data | no (auto: drop the 3 tables) | lossy (structure restorable, data lost) | yes | yes |
| `V2023_04_18__21_30__tickets-convert-utf8mb4` | `CONVERT TO CHARACTER SET utf8mb4` on `tickets` and `agents`; set table comment/row format | yes | lossy (back-conversion mangles/rejects non-latin1 chars; `TEXT` was promoted to `MEDIUMTEXT` and must be put back by hand) | yes | yes |
| `V2023_08_29__14_15__narrow-ticket-subject` | Move long subjects into body, `MODIFY subject varchar(255) → varchar(64)` | no | lossy (width restorable; original long subjects only survive inside `body`) | **no** (writers sending >64 chars fail) | yes (new code already caps at 64) |
| `V2023_11_06__10_40__rename-agents-to-staff` | `RENAME TABLE agents → staff_members`, add `role` enum column | yes | lossy (role assignments lost) | **no** (old code queries `agents`) | **no** (new code queries `staff_members`) |
| `V2024_02_12__08_05__auto-close-procedure` | Create `sp_auto_close_solved` procedure (`DELIMITER $$`); drop `ticket_legacy_tags` | yes (recreates the table empty) | lossy (tag data lost) | yes (nothing has read the tag table since HD-99) | yes for the drop; **no** for code that `CALL`s the procedure |
| `V2024_05_20__17_50__drop-tickets-requester-email` | Create `requesters`, migrate emails, add `tickets.requester_id` + FK, drop `tickets.requester_email` | no | **irreversible** as written (needs a reverse data migration to rebuild `requester_email`) | **no** (column drop) | **no** (new code requires `requester_id`/`requesters`) |

Notes:
- Mixed reversibility on purpose: the first undo file restores structure only after an extra
  `MODIFY` because `CONVERT TO utf8mb4` silently widens `TEXT` → `MEDIUMTEXT`.
- `requester_email` had a prefix index (`requester_email(20)`), dropped in the last migration.

Validated against MySQL 8.4: all ups apply in order; every undo applies after its up and
restores the pre-migration schema exactly (per `mysqldump --no-data --routines --triggers`);
the procedure was called and closed tickets as expected.

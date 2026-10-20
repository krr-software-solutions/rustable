# ex-2 — web shop orders (8 migrations)

A shop with `customers` and `orders`, imported from a legacy POS system. Over five
months the schema gets an index rework, a column rename + narrowing, a compliance
audit trigger, a NOT NULL tightening, a column drop, and a denormalized stats table.

Exercises: `/*!40101 … */` version comments and `@OLD_CHARACTER_SET_CLIENT` save/restore,
per-column `CHARACTER SET ascii COLLATE ascii_bin`, prefix index `email(20)`, enums,
`INSERT … SELECT`, `INSERT … SELECT … ON DUPLICATE KEY UPDATE VALUES()` (deprecated syntax, still a warning only),
descending index parts, `ALGORITHM=INPLACE, LOCK=NONE` / `LOCK=SHARED` / `ALGORITHM=COPY`,
`CHANGE COLUMN` (rename + retype), `MODIFY COLUMN … NOT NULL`, multi-clause `ALTER TABLE`,
`SET FOREIGN_KEY_CHECKS=0/1`, and a trigger wrapped in `DELIMITER $$ … $$ DELIMITER ;`.

| Migration | What it does | Undo file | Reversible | Backward compatible | Forward compatible |
|---|---|---|---|---|---|
| `V2024_09_02__08_00__create-customers` | Create `customers`, seed 2 rows | yes | lossy (structure restorable, data lost) | yes | yes |
| `V2024_09_09__16_20__create-orders` | Create `orders` (FK → customers), backfill one order per customer via `INSERT … SELECT` | no (auto: `DROP TABLE orders`) | lossy | yes | yes |
| `V2024_10_01__11_45__orders-customer-created-index` | Add `(customer_id, created_at DESC)` index; **drop `idx_orders_status`**, add `(status, created_at)` | yes — **intentionally WRONG** (see below) | lossless in principle (index-only) | yes | yes |
| `V2024_10_15__09_10__rename-customers-phone` | Clean up data, `CHANGE COLUMN phone → phone_number varchar(32→20)` | yes | lossy (name/width restored; stripped/truncated phone data not) | **no** (rename + narrowing) | **no** (new code reads `phone_number`) |
| `V2024_11_04__13_30__order-status-audit-trigger` | Create `order_status_audit` + `AFTER UPDATE` trigger (`DELIMITER $$`) | yes | lossy (audit rows lost) | yes | yes |
| `V2024_12_02__10_00__customers-email-not-null` | Backfill NULL emails, `MODIFY email … NOT NULL` | no (auto-gen not trivial: needs prior column def) | lossy (NULL-ness can be restored, synthetic emails stay) | **no** (old code inserting without email fails) | yes (new code always sends email) |
| `V2025_01_13__15_25__drop-orders-legacy-ref` | Drop index + drop column `orders.legacy_ref` | no | **irreversible** (data gone; structure could be re-added) | **no** (old code may still read it) | yes (new code no longer uses it) |
| `V2025_02_03__07_55__create-customer-order-stats` | Create `customer_order_stats`, populate via `INSERT … SELECT … GROUP BY … ON DUPLICATE KEY UPDATE` | yes | lossless-ish (data is fully derivable from `orders`) | yes | yes |

## Intentionally wrong undo

`V2024_10_01__11_45__orders-customer-created-index.undo.sql` drops the two indexes the up
added but **does not re-create `idx_orders_status`**, which the up dropped. It executes
fine, but the schema after `up → undo` differs from the schema before `up` (missing
`KEY idx_orders_status (status)`). Consequence (verified): re-applying the up after the undo
fails with `ERROR 1091: Can't DROP 'idx_orders_status'`. A tool comparing schema before/after
an up+undo round trip should flag this.

Validated against MySQL 8.4: all ups apply in order; every undo applies after its up; all
undos except the one above restore the pre-migration schema exactly; trigger and data
migrations exercised and behave as described.

# ex-2 — Ledgerly (multi-tenant invoicing SaaS), 10 migrations

Tenants own invoices in a separate `billing` schema. Over six months the team adds enum
values, an `updated_at` trigger, a partitioned audit table, row level security, a due date
with a validated check, reporting views, extracts customers out of a JSON blob, and finally
drops that JSON blob.

Features exercised: `CREATE EXTENSION IF NOT EXISTS pgcrypto`, `uuid DEFAULT gen_random_uuid()`,
`CREATE SCHEMA`, `CREATE TYPE … AS ENUM`, `ALTER TYPE … ADD VALUE [IF NOT EXISTS] [AFTER]`,
`jsonb`, `char(3)`, composite `UNIQUE`, `COMMENT ON SCHEMA/COLUMN`, plpgsql function with
`$$` body + `BEFORE UPDATE` trigger, `GENERATED ALWAYS AS IDENTITY`, `PARTITION BY RANGE` +
`PARTITION OF … FOR VALUES` / `DEFAULT`, `DO $$ … $$` block, `CREATE ROLE`, `GRANT/REVOKE`,
`ENABLE ROW LEVEL SECURITY`, `CREATE POLICY … USING … WITH CHECK`, `ADD CONSTRAINT … NOT VALID`
+ `VALIDATE CONSTRAINT` (both CHECK and FK), data backfill `UPDATE`, `UPDATE … FROM`,
`INSERT … SELECT DISTINCT ON`, `CREATE VIEW`, `CREATE MATERIALIZED VIEW … WITH DATA` + unique
index, `DROP COLUMN`, `DROP SCHEMA … CASCADE`.

| # | Migration | What it does | Undo file | Reversible | Backward compatible | Forward compatible |
|---|-----------|--------------|-----------|------------|---------------------|--------------------|
| 1 | `V2025_04_01__09_00__create-tenants` | pgcrypto extension + `tenants` (uuid PK) | no (auto: `DROP TABLE tenants`; extension is ambiguous to undo) | lossy | yes | no |
| 2 | `V2025_04_08__11_30__create-billing-invoices` | Schema `billing`, enum `invoice_status`, `billing.invoices` | yes (`DROP SCHEMA billing CASCADE`) | lossy | yes | no |
| 3 | `V2025_04_22__15_10__add-invoice-status-values` | `ALTER TYPE … ADD VALUE 'void'`, `'uncollectible' AFTER 'paid'` | no | **irreversible** (Postgres cannot drop enum values) | yes (old code never writes the new values; readers that exhaustively match may break) | no (new code writes `void`) |
| 4 | `V2025_05_05__08_45__add-invoices-updated-at-trigger` | Add `updated_at NOT NULL DEFAULT now()`, plpgsql function, trigger | yes | lossy (timestamps lost) | yes (has a default) | no |
| 5 | `V2025_05_19__13_20__create-invoice-events-partitioned` | Range-partitioned `billing.invoice_events` with Q2/Q3/default partitions | no (auto: `DROP TABLE billing.invoice_events` drops partitions too) | lossy | yes | no |
| 6 | `V2025_06_02__10_00__enable-invoices-rls` | `DO $$` create role if missing, grants, enable RLS, tenant policy | yes (role intentionally left behind) | lossless for the table; role not removed | **no** for code connecting as `ledgerly_app` without setting `app.tenant_id` (sees zero rows) | yes (code that sets `app.tenant_id` works either way) |
| 7 | `V2025_06_16__14_40__add-invoices-due-date` | Add nullable `due_date`, backfill, CHECK `NOT VALID` then `VALIDATE` | yes | lossy (due dates lost) | yes | no |
| 8 | `V2025_07_07__09_15__create-reporting-views` | View `open_invoices`, materialized view `monthly_revenue` + unique index | yes | lossless (views hold no source data) | yes | no |
| 9 | `V2025_08_11__16_05__extract-billing-customers` | New `billing.customers`, `INSERT … SELECT` from JSON, add + backfill `customer_id`, FK `NOT VALID` + `VALIDATE` | no (auto would need: drop FK, drop column, drop table) | lossy (structure restorable; data still recoverable from `metadata` until #10) | yes (nullable column, new table) | no |
| 10 | `V2025_09_01__10_30__drop-invoices-metadata` | `DROP COLUMN metadata` | yes (re-adds `jsonb NOT NULL DEFAULT '{}'`) | lossy (structure restorable, JSON data lost) | **no** (old code reads/writes `metadata`) | yes (new code no longer uses it) |

Notes:
- **Parser gotcha:** migration 6 contains a `DO $$ … $$` block; `sqlparser` 0.63 is known to fail
  on these. Migration 4 also has a `$$`-quoted function body.
- Migration 3 is the canonical "cannot be undone" case: there is no `ALTER TYPE … DROP VALUE`.
- Migration 6's undo is *deliberately* partial (does not drop the cluster-wide role) — this is
  a judgement call, not a bug; redoing migration 6 after the undo still works thanks to the `DO` guard.
- Migration 8's view lists explicit columns (not `i.*`), which is what allows migration 10 to drop
  `metadata` without `CASCADE`.

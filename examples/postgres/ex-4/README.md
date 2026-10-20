# ex-4 — Stockroom (warehouse inventory), 6 migrations

Inventory for a few warehouses: products, per-warehouse stock levels, a SKU rollout,
a cents-to-decimal price conversion, dropping a legacy table and a naming cleanup.

Features exercised: `integer`/`bigint GENERATED ALWAYS AS IDENTITY`, `bigserial`, `char(4)`,
`CHECK` constraints (column and named), FK with and without `ON DELETE CASCADE`, composite
primary key, partial index, `COMMENT ON TABLE`, seed `INSERT`, backfill `UPDATE`,
`SET NOT NULL` on an existing column, `CREATE UNIQUE INDEX CONCURRENTLY`, expression index,
`DROP CONSTRAINT`, `SET DATA TYPE numeric(12,2) USING …`, `RENAME COLUMN`, `DROP TABLE`,
`ALTER TABLE … RENAME TO`, `ALTER INDEX … RENAME TO`, `SET DEFAULT` / `DROP DEFAULT`,
`DROP NOT NULL`.

| # | Migration | What it does | Undo file | Reversible | Backward compatible | Forward compatible |
|---|-----------|--------------|-----------|------------|---------------------|--------------------|
| 1 | `V2025_02_10__09_00__create-warehouses-and-products` | Create `warehouses`, `products`, `warehouse_notes`; seed rows | yes | lossy | yes | no |
| 2 | `V2025_02_24__13_15__create-stock-levels` | Create `stock_levels` (composite PK, cascading FKs, partial index) | no (auto: `DROP TABLE stock_levels`) | lossy | yes | no |
| 3 | `V2025_03_17__10_40__add-products-sku` | Add `sku`, backfill, `SET NOT NULL`, unique index + `lower(name)` index (both `CONCURRENTLY`) | yes — **intentionally wrong** | lossy (SKUs lost) | **no** (old code inserting without `sku` fails NOT NULL) | no |
| 4 | `V2025_04_28__15_00__convert-price-to-numeric` | `price_cents bigint` → `price numeric(12,2)` via `USING /100.0`, rename, swap CHECK | yes | lossless (cents ↔ 2-decimal values round-trip exactly) | **no** (column renamed and unit changed) | **no** |
| 5 | `V2025_05_12__08_30__drop-warehouse-notes` | `DROP TABLE warehouse_notes` | no | **irreversible** (structure could be guessed, data gone; no undo supplied) | **no** (if any old code still reads it) | yes |
| 6 | `V2025_06_09__17_20__rename-stock-levels-to-inventory` | `stock_levels` → `inventory` (+ index rename), `quantity SET DEFAULT 0`, `updated_at DROP DEFAULT`, `products.description DROP NOT NULL / DROP DEFAULT` + data `UPDATE` | yes | lossless (`''` ↔ `NULL` mapping is 1:1) | **no** (table renamed; also old inserts relying on `updated_at` default now get NULL→NOT NULL error) | **no** |

**Intentionally wrong undo:** `V2025_03_17__10_40__add-products-sku.undo.sql` only drops the
`sku` column (which implicitly drops `products_sku_key`) but **forgets to drop
`products_name_lower_idx`**. It executes successfully; however re-applying the up migration
afterwards fails with `relation "products_name_lower_idx" already exists` (verified).

Notes:
- Migration 3 mixes transactional statements with `CREATE INDEX CONCURRENTLY`; it must run
  outside a single wrapping transaction.
- Migration 4 drops the auto-named constraint `products_price_cents_check` — a tool deriving
  schema state must know Postgres' default constraint naming (`<table>_<column>_check`).

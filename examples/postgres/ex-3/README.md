# ex-3 — Mise (personal recipe box), 2 migrations

A tiny recipe app. The first migration creates the table and seeds two recipes; the second
narrows column types to match the mobile UI.

Features exercised: `GENERATED ALWAYS AS IDENTITY`, `text[]` with `ARRAY[]::text[]` default,
`jsonb` + GIN `jsonb_path_ops` index, unique expression index, seed `INSERT`,
`ALTER COLUMN … SET DATA TYPE … USING …` (type **narrowing**: `text → varchar(120)`,
`integer → smallint`), multi-action `ALTER TABLE`, `CHECK` constraint.

| # | Migration | What it does | Undo file | Reversible | Backward compatible | Forward compatible |
|---|-----------|--------------|-----------|------------|---------------------|--------------------|
| 1 | `V2024_11_12__19_00__create-recipes` | Create `recipes`, two indexes, seed 2 rows | yes | lossy (drop table) | yes | no |
| 2 | `V2025_01_04__11_25__narrow-recipe-columns` | `name` → `varchar(120)` (truncating), `servings` → `smallint`, add `servings <= 100` | yes — **intentionally wrong** | lossy (names > 120 chars truncated; structure restorable) | **no** (old code inserting long names / servings > 100 now fails) | yes (new code that respects the limits works on the wider types) |

**Intentionally wrong undo:** `V2025_01_04__11_25__narrow-recipe-columns.undo.sql` restores
`name` as `varchar(255)` instead of the original `text`. It executes fine, and `servings` is
correctly restored to `integer`, but the resulting schema differs from the pre-migration schema.
A schema-diffing check (state after `up`+`undo` vs. state before `up`) should flag
`recipes.name: text != varchar(255)`.

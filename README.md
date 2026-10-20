# rustable

SQL schema migrations, in Rust — in the spirit of [Flyway](https://flywaydb.org/), for
**Postgres**, **MySQL**, and **SQLite**.

Point it at a directory of plain `.sql` migrations and it will:

- parse them (statement by statement, recovering from anything `sqlparser` can't handle),
- fold them into the schema they produce (`schema.toml`) and a readable `MIGRATIONS.md`,
- tell you, per migration, whether it's **reversible** and **forwards/backwards compatible**
  (i.e. whether to run it before or after deploying code),
- check every `.undo.sql` actually restores the schema, and **generate** trivial ones,
- diff the schema against tables declared in Rust (`#[derive(Table)]`) and **plan** a migration,
  optionally with soft drops,
- apply migrations to a real database, and **verify** the real database matches what it derived.

## Migrations

Files live next to `config.toml` (or wherever `migrations_dir_path` points):

```
VYYYY_MM_DD__HH_MM__kebab-case-name.sql        the migration
VYYYY_MM_DD__HH_MM__kebab-case-name.undo.sql   its undo (optional)
```

```toml
# config.toml — every path is relative to this file
dialect = "postgres"                                  # postgres | mysql | sqlite
# migrations_dir_path = "."
# current_schema_path = "schema.toml"
# migrations_md_path = "MIGRATIONS.md"
# code_schema_path = "code.current_schema.toml"
# outstanding_path = "OUTSTANDING.md"
```

## Commands

Every command takes `--config-path` (or `RUSTABLE_CONFIG_PATH`).

| command | what it does |
|---|---|
| `rustable analyze` | per migration: deploy phase, reversibility, compatibility, undo status |
| `rustable generate [--no-undo]` | write trivial `.undo.sql` files, then `schema.toml` and `MIGRATIONS.md` |
| `rustable check` | fail if `schema.toml`/`MIGRATIONS.md` are stale or any `.undo.sql` doesn't restore the schema (for CI) |
| `rustable diff` | how the code's tables (`code.current_schema.toml`) differ from the migrations |
| `rustable plan [--soft-drops] [--write]` | a migration that brings the migrations in line with the code; leftovers go to `OUTSTANDING.md` |
| `rustable db status\|migrate\|undo --to <version>\|verify` | run against a real database: `--db-host --db-port --db-user --db-password --db-name` (or `RUSTABLE_DB_*` env vars; SQLite: `--db-name` is the file) |
| `rustable completions <shell>` | shell completion script |

### Running on app startup

`db migrate`/`undo`/`verify`/`status` take a lock first (Postgres advisory lock, MySQL
`GET_LOCK`; SQLite: pass `--lock-file`), so many instances can run `rustable db migrate` at once:
one migrates, the others wait (up to `--lock-timeout`, default 300s) and then find nothing to do.
Everything rustable does is appended to the `rustable_events` table (`*_started`, `*_succeeded`,
`*_failed`): audit only, never read back. `rustable_history` holds what's currently applied.

### Deploy phases

- **expand** — backward compatible: old code still works after it. Run it *before* deploying.
- **contract** — forward compatible: new code also works before it. Run it *after* deploying.
- **any order** — both. **breaking** — neither: needs a coordinated deploy.

### Reversibility

**lossless** · **conditional** (undo may fail on data written since) · **lossy** (structure
comes back, data doesn't) · **irreversible** (data changes, unknown statements, enum values).

## Tables in code

```rust
#[derive(rustable_derive::Table)]
#[rustable(table = "bookmarks", index = "bookmarks_live_idx: user_id, created_at DESC where archived_at IS NULL")]
struct Bookmark {
    #[rustable(primary_key, sql_type = "BIGSERIAL")]
    id: i64,
    #[rustable(indexed, references = "users(id)", on_delete = "CASCADE")]
    user_id: i64,
    url: String,
    archived_at: Option<std::time::SystemTime>,   // Option = nullable
    #[rustable(default = "now()")]
    created_at: std::time::SystemTime,
}

// e.g. in a test or a small bin:
rustable_core::code::write_code_schema("code.current_schema.toml".as_ref(), Dialect::Postgres)?;
```

See `code/example-app` (mirrors `examples/postgres/ex-1`, with deliberate drift).

## Layout

```
code/core         rustable-core: the whole analysis pipeline (no I/O beyond reading files)
  version.rs        file names → MigrationId (version + name)
  discover.rs       find + pair up/undo files
  statements.rs     split a file into statements, parse each (with recovery)
  heuristics.rs     fallbacks for what sqlparser can't parse (DELIMITER, MySQL options, …)
  schema.rs         the schema model (= schema.toml)
  change.rs         one effect of a statement, with before-snapshots; apply_to / inverse
  apply.rs          statement → changes (the fold)
  analysis.rs       reversibility + compatibility per change
  undo.rs           inverse SQL; trivial undo generation
  diff.rs, plan.rs  schema vs schema; schema → migration (soft drops)
  project.rs        analyze a whole directory; check undos
  report.rs         MIGRATIONS.md
  code.rs           #[derive(Table)] support: SqlType, registry, code.current_schema.toml
code/derive       rustable-derive: the #[derive(Table)] proc macro
code/db           rustable-db: apply to a real database via sqlx (history table, verify)
code/cli          rustable: the CLI
code/example-app  tables declared in Rust, for the diff/plan demo
examples/         12 example projects (4 mysql, 5 postgres, 3 sqlite3), validated against real DBs
```

`cargo test` runs the analyzer over every example: it must flag exactly the four deliberately
broken undos documented in the examples' READMEs.

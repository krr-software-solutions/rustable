# rustable

SQL schema migrations, in Rust — in the spirit of [Flyway](https://flywaydb.org/).

Point it at a directory of `.sql` migration files and it will:

- parse them,
- derive the resulting schema,
- and run the pending migrations against your database.

> **Status:** early days — nothing to see here yet.

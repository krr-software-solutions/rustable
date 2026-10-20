//! Render parts of the schema model back into SQL, per dialect.

use crate::dialect::Dialect;
use crate::names::unqualified;
use crate::schema::{Check, Column, ForeignKey, Index, Table, View};

/// A SQL string literal.
#[must_use]
pub fn literal(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// `name type [NOT NULL] [DEFAULT …] …` as it would appear in `CREATE TABLE` / `ADD COLUMN`.
#[must_use]
pub fn column_def(column: &Column, dialect: Dialect) -> String {
    let mut out = format!("{} {}", dialect.quote(&column.name), column.data_type);
    if let Some(charset) = &column.charset {
        out.push_str(&format!(" CHARACTER SET {charset}"));
    }
    if let Some(collation) = &column.collation {
        out.push_str(&format!(" COLLATE {collation}"));
    }
    if let Some(generated) = &column.generated {
        out.push(' ');
        out.push_str(generated);
    }
    if !column.nullable && !is_serial(&column.data_type) {
        out.push_str(" NOT NULL");
    }
    if let Some(default) = &column.default {
        out.push_str(&format!(" DEFAULT {default}"));
    }
    if column.auto_increment && dialect == Dialect::MySql {
        out.push_str(" AUTO_INCREMENT");
    }
    if let Some(on_update) = &column.on_update {
        out.push_str(&format!(" ON UPDATE {on_update}"));
    }
    if let (Some(comment), Dialect::MySql) = (&column.comment, dialect) {
        out.push_str(&format!(" COMMENT {}", literal(comment)));
    }
    out
}

fn is_serial(data_type: &str) -> bool {
    data_type.to_ascii_uppercase().ends_with("SERIAL")
}

fn columns(cols: &[String], dialect: Dialect) -> String {
    cols.iter()
        .map(|c| {
            // Plain names get quoted if needed; expressions (`lower(email)`, `x DESC`) pass through.
            if c.chars().all(|ch| ch.is_alphanumeric() || ch == '_') {
                dialect.quote(c)
            } else {
                c.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[must_use]
pub fn foreign_key_clause(fk: &ForeignKey, dialect: Dialect) -> String {
    let mut out = String::new();
    if let Some(name) = &fk.name {
        out.push_str(&format!("CONSTRAINT {} ", dialect.quote(name)));
    }
    out.push_str(&format!(
        "FOREIGN KEY ({}) REFERENCES {} ({})",
        columns(&fk.columns, dialect),
        dialect.quote(&fk.references_table),
        columns(&fk.references_columns, dialect)
    ));
    if let Some(action) = &fk.on_delete {
        out.push_str(&format!(" ON DELETE {action}"));
    }
    if let Some(action) = &fk.on_update {
        out.push_str(&format!(" ON UPDATE {action}"));
    }
    out
}

#[must_use]
pub fn check_clause(check: &Check, dialect: Dialect) -> String {
    match &check.name {
        Some(name) => format!("CONSTRAINT {} CHECK ({})", dialect.quote(name), check.expr),
        None => format!("CHECK ({})", check.expr),
    }
}

fn unique_clause(index: &Index, dialect: Dialect) -> String {
    match &index.name {
        Some(name) => format!(
            "CONSTRAINT {} UNIQUE ({})",
            dialect.quote(name),
            columns(&index.columns, dialect)
        ),
        None => format!("UNIQUE ({})", columns(&index.columns, dialect)),
    }
}

/// `CREATE [UNIQUE] INDEX …` (or an `ADD CONSTRAINT … UNIQUE` for constraint-backed indexes).
#[must_use]
pub fn create_index(table: &str, index: &Index, dialect: Dialect) -> Option<String> {
    if index.constraint {
        if dialect == Dialect::Sqlite {
            return None; // SQLite can't add constraints after the fact
        }
        return Some(format!(
            "ALTER TABLE {} ADD {}",
            dialect.quote(table),
            unique_clause(index, dialect)
        ));
    }
    let name = index.name.as_ref()?;
    let using = match (&index.using, dialect) {
        (Some(u), Dialect::MySql) if u == "FULLTEXT" || u == "SPATIAL" => {
            return Some(format!(
                "CREATE {u} INDEX {} ON {} ({})",
                dialect.quote(name),
                dialect.quote(table),
                columns(&index.columns, dialect)
            ));
        }
        (Some(u), Dialect::Postgres) => format!(" USING {u}"),
        _ => String::new(),
    };
    let mut out = format!(
        "CREATE {}INDEX {} ON {}{using} ({})",
        if index.unique { "UNIQUE " } else { "" },
        dialect.quote(unqualified(name)),
        dialect.quote(table),
        columns(&index.columns, dialect)
    );
    if let Some(predicate) = &index.predicate {
        out.push_str(&format!(" WHERE {predicate}"));
    }
    Some(out)
}

/// `DROP INDEX …` for an index (or `DROP CONSTRAINT` for constraint-backed ones).
#[must_use]
pub fn drop_index(table: &str, index: &Index, dialect: Dialect) -> Option<String> {
    let name = index.name.as_ref()?;
    Some(match dialect {
        Dialect::MySql => format!(
            "DROP INDEX {} ON {}",
            dialect.quote(name),
            dialect.quote(table)
        ),
        Dialect::Postgres if index.constraint => format!(
            "ALTER TABLE {} DROP CONSTRAINT {}",
            dialect.quote(table),
            dialect.quote(name)
        ),
        Dialect::Postgres => format!("DROP INDEX {}", dialect.quote(&qualify_like(table, name))),
        Dialect::Sqlite if index.constraint => return None,
        Dialect::Sqlite => format!("DROP INDEX {}", dialect.quote(name)),
    })
}

/// Put `name` in the same schema as `table` (`billing.invoices` + `idx` → `billing.idx`).
fn qualify_like(table: &str, name: &str) -> String {
    match table.rsplit_once('.') {
        Some((schema, _)) if !name.contains('.') => format!("{schema}.{name}"),
        _ => name.to_string(),
    }
}

/// A full `CREATE TABLE`, followed by `CREATE INDEX` statements for its plain indexes.
#[must_use]
pub fn create_table(table: &Table, dialect: Dialect) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let sqlite_inline_pk = dialect == Dialect::Sqlite
        && table.primary_key.len() == 1
        && table
            .column(&table.primary_key[0])
            .is_some_and(|c| c.auto_increment);

    for column in &table.columns {
        let mut def = column_def(column, dialect);
        if sqlite_inline_pk && column.name == table.primary_key[0] {
            def.push_str(" PRIMARY KEY AUTOINCREMENT");
        }
        lines.push(def);
    }
    if !table.primary_key.is_empty() && !sqlite_inline_pk {
        lines.push(format!(
            "PRIMARY KEY ({})",
            columns(&table.primary_key, dialect)
        ));
    }
    let mut after = Vec::new();
    for index in &table.indexes {
        if index.constraint {
            lines.push(unique_clause(index, dialect));
        } else if dialect == Dialect::MySql {
            let kind = match index.using.as_deref() {
                Some("FULLTEXT") => "FULLTEXT KEY",
                Some("SPATIAL") => "SPATIAL KEY",
                _ if index.unique => "UNIQUE KEY",
                _ => "KEY",
            };
            let name = index
                .name
                .as_ref()
                .map(|n| format!("{} ", dialect.quote(n)))
                .unwrap_or_default();
            lines.push(format!(
                "{kind} {name}({})",
                columns(&index.columns, dialect)
            ));
        } else if let Some(sql) = create_index(&table.name, index, dialect) {
            after.push(sql);
        }
    }
    for fk in &table.foreign_keys {
        lines.push(foreign_key_clause(fk, dialect));
    }
    for check in &table.checks {
        lines.push(check_clause(check, dialect));
    }

    let mut sql = format!(
        "CREATE TABLE {} (\n    {}\n)",
        dialect.quote(&table.name),
        lines.join(",\n    ")
    );
    let mut options: Vec<String> = Vec::new();
    for (key, value) in &table.options {
        match (dialect, key.as_str()) {
            (Dialect::Sqlite, "STRICT" | "WITHOUT ROWID") => options.push(key.clone()),
            (Dialect::Postgres, "PARTITION BY") => options.push(format!("PARTITION BY {value}")),
            (Dialect::MySql, _) => options.push(format!("{key}={value}")),
            _ => {}
        }
    }
    if let (Some(comment), Dialect::MySql) = (&table.comment, dialect) {
        options.push(format!("COMMENT={}", literal(comment)));
    }
    if !options.is_empty() {
        let sep = if dialect == Dialect::Sqlite {
            ", "
        } else {
            " "
        };
        sql.push(' ');
        sql.push_str(&options.join(sep));
    }
    let mut out = vec![sql];
    out.extend(after);
    out
}

#[must_use]
pub fn create_view(view: &View, dialect: Dialect, or_replace: bool) -> String {
    format!(
        "CREATE {}{}VIEW {} AS {}",
        if or_replace && !view.materialized && dialect != Dialect::Sqlite {
            "OR REPLACE "
        } else {
            ""
        },
        if view.materialized {
            "MATERIALIZED "
        } else {
            ""
        },
        dialect.quote(&view.name),
        view.query
    )
}

#[must_use]
pub fn drop_view(view: &View, dialect: Dialect) -> String {
    format!(
        "DROP {}VIEW {}",
        if view.materialized {
            "MATERIALIZED "
        } else {
            ""
        },
        dialect.quote(&view.name)
    )
}

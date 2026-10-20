//! Undo SQL: the inverse of each change, and auto-generated `.undo.sql` files.

use std::collections::BTreeSet;

use crate::analysis::{Reversibility, reversibility};
use crate::change::Change;
use crate::dialect::Dialect;
use crate::render;
use crate::schema::Column;

/// SQL that reverses `change`, or `None` if rustable can't render it for this dialect.
///
/// For lossy changes (`DROP COLUMN`, `DROP TABLE`) this restores the *structure* only.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn inverse_sql(change: &Change, dialect: Dialect) -> Option<Vec<String>> {
    let q = |name: &str| dialect.quote(name);
    let one = |sql: String| Some(vec![sql]);
    match change {
        Change::CreateTable { table } => one(format!("DROP TABLE {}", q(&table.name))),
        Change::DropTable { table } => Some(render::create_table(table, dialect)),
        Change::RenameTable { from, to } => one(match dialect {
            Dialect::MySql => format!("RENAME TABLE {} TO {}", q(to), q(from)),
            Dialect::Postgres | Dialect::Sqlite => {
                format!(
                    "ALTER TABLE {} RENAME TO {}",
                    q(to),
                    q(crate::names::unqualified(from))
                )
            }
        }),
        Change::AddColumn { table, column, .. } => one(format!(
            "ALTER TABLE {} DROP COLUMN {}",
            q(table),
            q(&column.name)
        )),
        Change::DropColumn { table, column, .. } => one(format!(
            "ALTER TABLE {} ADD COLUMN {}",
            q(table),
            render::column_def(column, dialect)
        )),
        Change::RenameColumn { table, from, to } => one(format!(
            "ALTER TABLE {} RENAME COLUMN {} TO {}",
            q(table),
            q(to),
            q(from)
        )),
        Change::AlterColumn {
            table,
            before,
            after,
        } => restore_column(table, before, after, dialect),
        Change::SetPrimaryKey { .. } => None,
        Change::AddIndex { table, index } => {
            render::drop_index(table, index, dialect).map(|s| vec![s])
        }
        Change::DropIndex { table, index } => {
            render::create_index(table, index, dialect).map(|s| vec![s])
        }
        Change::AddForeignKey { table, fk } => {
            let name = fk.name.as_ref()?;
            match dialect {
                Dialect::MySql => one(format!(
                    "ALTER TABLE {} DROP FOREIGN KEY {}",
                    q(table),
                    q(name)
                )),
                Dialect::Postgres => one(format!(
                    "ALTER TABLE {} DROP CONSTRAINT {}",
                    q(table),
                    q(name)
                )),
                Dialect::Sqlite => None,
            }
        }
        Change::DropForeignKey { table, fk } => match dialect {
            Dialect::Sqlite => None,
            _ => one(format!(
                "ALTER TABLE {} ADD {}",
                q(table),
                render::foreign_key_clause(fk, dialect)
            )),
        },
        Change::AddCheck { table, check } => {
            let name = check.name.as_ref()?;
            match dialect {
                Dialect::MySql => one(format!("ALTER TABLE {} DROP CHECK {}", q(table), q(name))),
                Dialect::Postgres => one(format!(
                    "ALTER TABLE {} DROP CONSTRAINT {}",
                    q(table),
                    q(name)
                )),
                Dialect::Sqlite => None,
            }
        }
        Change::DropCheck { table, check } => match dialect {
            Dialect::Sqlite => None,
            _ => one(format!(
                "ALTER TABLE {} ADD {}",
                q(table),
                render::check_clause(check, dialect)
            )),
        },
        Change::RenameConstraint { table, from, to } => match dialect {
            Dialect::Postgres => one(format!(
                "ALTER TABLE {} RENAME CONSTRAINT {} TO {}",
                q(table),
                q(to),
                q(from)
            )),
            Dialect::MySql => one(format!(
                "ALTER TABLE {} RENAME INDEX {} TO {}",
                q(table),
                q(to),
                q(from)
            )),
            Dialect::Sqlite => None,
        },
        Change::SetTableOptions {
            table,
            before,
            after,
            convert,
        } => {
            if dialect != Dialect::MySql {
                return None;
            }
            let mut parts = Vec::new();
            if *convert {
                let charset = before.get("DEFAULT CHARSET")?;
                let mut part = format!("CONVERT TO CHARACTER SET {charset}");
                if let Some(collate) = before.get("COLLATE") {
                    part.push_str(&format!(" COLLATE {collate}"));
                }
                parts.push(part);
            }
            for (key, value) in before {
                let skip = *convert && (key == "DEFAULT CHARSET" || key == "COLLATE");
                if !skip && after.get(key) != Some(value) {
                    parts.push(format!("{key}={value}"));
                }
            }
            if parts.is_empty() {
                Some(vec![])
            } else {
                one(format!("ALTER TABLE {} {}", q(table), parts.join(", ")))
            }
        }
        Change::SetRowLevelSecurity { table, enabled } => match dialect {
            Dialect::Postgres => one(format!(
                "ALTER TABLE {} {} ROW LEVEL SECURITY",
                q(table),
                if *enabled { "DISABLE" } else { "ENABLE" }
            )),
            _ => None,
        },
        Change::SetComment {
            table,
            column,
            before,
            ..
        } => {
            let value = before
                .as_deref()
                .map_or_else(|| "NULL".to_string(), render::literal);
            match (dialect, column) {
                (Dialect::Postgres, None) => {
                    one(format!("COMMENT ON TABLE {} IS {value}", q(table)))
                }
                (Dialect::Postgres, Some(c)) => one(format!(
                    "COMMENT ON COLUMN {}.{} IS {value}",
                    q(table),
                    q(c)
                )),
                (Dialect::MySql, None) => one(format!(
                    "ALTER TABLE {} COMMENT={}",
                    q(table),
                    render::literal(before.as_deref().unwrap_or(""))
                )),
                _ => None,
            }
        }
        Change::CreateView { view, replaced } => match replaced {
            None => one(render::drop_view(view, dialect)),
            Some(old) if dialect == Dialect::Sqlite || old.materialized => Some(vec![
                render::drop_view(view, dialect),
                render::create_view(old, dialect, false),
            ]),
            Some(old) => one(render::create_view(old, dialect, true)),
        },
        Change::DropView { view } => one(render::create_view(view, dialect, false)),
        Change::CreateEnum { enum_type } => one(format!("DROP TYPE {}", q(&enum_type.name))),
        Change::DropEnum { enum_type } => one(format!(
            "CREATE TYPE {} AS ENUM ({})",
            q(&enum_type.name),
            enum_type
                .values
                .iter()
                .map(|v| render::literal(v))
                .collect::<Vec<_>>()
                .join(", ")
        )),
        Change::CreateObject {
            object,
            replaced: false,
        } => {
            let name = q(&object.name);
            match (object.kind.as_str(), &object.table, dialect) {
                ("trigger", Some(table), Dialect::Postgres) => {
                    one(format!("DROP TRIGGER {name} ON {}", q(table)))
                }
                ("policy", Some(table), _) => one(format!("DROP POLICY {name} ON {}", q(table))),
                ("index", Some(_), Dialect::Postgres | Dialect::Sqlite) => {
                    one(format!("DROP INDEX {name}"))
                }
                ("virtual table", _, _) => one(format!("DROP TABLE {name}")),
                (
                    "extension" | "schema" | "function" | "trigger" | "sequence" | "type"
                    | "procedure" | "event",
                    _,
                    _,
                ) => one(format!("DROP {} {name}", object.kind.to_ascii_uppercase())),
                _ => None,
            }
        }
        Change::Session { .. } => Some(vec![]),
        Change::AddEnumValue { .. }
        | Change::CreateObject { replaced: true, .. }
        | Change::DropObject { .. }
        | Change::Data { .. }
        | Change::Unknown { .. } => None,
    }
}

/// Restore a column's definition after an `AlterColumn`.
fn restore_column(
    table: &str,
    before: &Column,
    after: &Column,
    dialect: Dialect,
) -> Option<Vec<String>> {
    let q = |name: &str| dialect.quote(name);
    match dialect {
        Dialect::MySql => Some(vec![if before.name == after.name {
            format!(
                "ALTER TABLE {} MODIFY COLUMN {}",
                q(table),
                render::column_def(before, dialect)
            )
        } else {
            format!(
                "ALTER TABLE {} CHANGE COLUMN {} {}",
                q(table),
                q(&after.name),
                render::column_def(before, dialect)
            )
        }]),
        Dialect::Postgres => {
            let mut out = Vec::new();
            let t = q(table);
            if before.name != after.name {
                out.push(format!(
                    "ALTER TABLE {t} RENAME COLUMN {} TO {}",
                    q(&after.name),
                    q(&before.name)
                ));
            }
            let c = q(&before.name);
            if before.data_type != after.data_type {
                out.push(format!(
                    "ALTER TABLE {t} ALTER COLUMN {c} SET DATA TYPE {}",
                    before.data_type
                ));
            }
            if before.nullable != after.nullable {
                out.push(format!(
                    "ALTER TABLE {t} ALTER COLUMN {c} {} NOT NULL",
                    if before.nullable { "DROP" } else { "SET" }
                ));
            }
            if before.default != after.default {
                out.push(match &before.default {
                    Some(d) => format!("ALTER TABLE {t} ALTER COLUMN {c} SET DEFAULT {d}"),
                    None => format!("ALTER TABLE {t} ALTER COLUMN {c} DROP DEFAULT"),
                });
            }
            let other_changed = Column {
                name: before.name.clone(),
                data_type: before.data_type.clone(),
                nullable: before.nullable,
                default: before.default.clone(),
                ..after.clone()
            } != *before;
            if other_changed { None } else { Some(out) }
        }
        Dialect::Sqlite => None, // needs a table rebuild
    }
}

/// SQL that performs `change` (rather than reversing it): the inverse of its inverse.
#[must_use]
pub fn forward_sql(change: &Change, dialect: Dialect) -> Option<Vec<String>> {
    match change {
        Change::AddEnumValue { name, value } if dialect == Dialect::Postgres => {
            Some(vec![format!(
                "ALTER TYPE {} ADD VALUE {}",
                dialect.quote(name),
                render::literal(value)
            )])
        }
        _ => inverse_sql(&change.inverse()?, dialect),
    }
}

/// True if `sql` is an `UPDATE` whose every assigned column is in `added` (for its table).
#[must_use]
pub fn backfills_new_columns(sql: &str, dialect: Dialect, added: &[(String, String)]) -> bool {
    let Some(table) = crate::statements::data_target(sql, dialect) else {
        return false;
    };
    match crate::statements::update_assigned_columns(sql, dialect) {
        Some(cols) if !cols.is_empty() => cols
            .iter()
            .all(|c| added.iter().any(|(t, a)| *t == table && a == c)),
        _ => false,
    }
}

/// The outcome of trying to auto-generate an undo for a migration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GeneratedUndo {
    /// The undo script.
    Generated(String),
    /// Why an undo isn't trivial for this migration.
    NotTrivial(Vec<String>),
}

/// Generate an undo script, if every change in `changes` has a lossless, renderable inverse.
#[must_use]
pub fn generate_undo(changes: &[Change], dialect: Dialect) -> GeneratedUndo {
    let mut reasons = Vec::new();
    let mut statements = Vec::new();

    // Indexes/constraints on a table created in this same migration disappear with the table,
    // so they don't need their own inverse.
    let created: BTreeSet<&str> = changes
        .iter()
        .filter_map(|c| match c {
            Change::CreateTable { table } => Some(table.name.as_str()),
            _ => None,
        })
        .collect();
    // Columns added by this migration: constraints on just those columns go when they're dropped.
    let added: BTreeSet<(&str, &str)> = changes
        .iter()
        .filter_map(|c| match c {
            Change::AddColumn { table, column, .. } => Some((table.as_str(), column.name.as_str())),
            _ => None,
        })
        .collect();
    let only_added = |table: &str, text: &[String]| {
        let words: Vec<&str> = text
            .iter()
            .flat_map(|t| t.split(|ch: char| !(ch.is_alphanumeric() || ch == '_')))
            .filter(|w| !w.is_empty())
            .collect();
        words.iter().any(|w| added.contains(&(table, *w)))
    };

    for change in changes.iter().rev() {
        let on_added_column = match change {
            Change::AddIndex { table, index } => only_added(table, &index.columns),
            Change::AddForeignKey { table, fk } => only_added(table, &fk.columns),
            Change::AddCheck { table, check } => {
                only_added(table, std::slice::from_ref(&check.expr))
            }
            _ => false,
        };
        let added_owned: Vec<(String, String)> = added
            .iter()
            .map(|(t, c)| ((*t).to_string(), (*c).to_string()))
            .collect();
        let seeds_new_table = match change {
            Change::Data { sql } => {
                crate::statements::data_target(sql, dialect)
                    .is_some_and(|t| created.contains(t.as_str()))
                    || backfills_new_columns(sql, dialect, &added_owned)
            }
            _ => false,
        };
        if on_added_column || seeds_new_table {
            continue;
        }
        let redundant = match change {
            Change::AddIndex { table, .. }
            | Change::AddForeignKey { table, .. }
            | Change::AddCheck { table, .. }
            | Change::AddColumn { table, .. }
            | Change::SetComment { table, .. }
            | Change::SetRowLevelSecurity { table, .. } => created.contains(table.as_str()),
            Change::CreateObject {
                object,
                replaced: false,
            } => object.table.as_deref().is_some_and(|t| created.contains(t)),
            _ => false,
        };
        if redundant {
            continue;
        }
        let rev = reversibility(change);
        if !matches!(rev, Reversibility::Lossless) {
            reasons.push(format!(
                "{change}: {} ({})",
                rev.label(),
                rev.reason().unwrap_or_default()
            ));
            continue;
        }
        match inverse_sql(change, dialect) {
            Some(sqls) => statements.extend(sqls),
            None => reasons.push(format!("{change}: no automatic inverse for {dialect}")),
        }
    }

    if !reasons.is_empty() {
        return GeneratedUndo::NotTrivial(reasons);
    }
    let mut script = String::from(
        "-- Generated by rustable: the inverse of the migration with the same name.\n",
    );
    for sql in statements {
        script.push('\n');
        script.push_str(&sql);
        script.push_str(";\n");
    }
    GeneratedUndo::Generated(script)
}

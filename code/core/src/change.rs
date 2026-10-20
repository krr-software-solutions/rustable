//! A `Change` is one effect a statement has on the schema.
//!
//! Changes carry a snapshot of whatever they remove or replace (the dropped column, the column
//! definition before an `ALTER`, …). That's what makes it possible to work out reversibility and
//! render an undo without looking anything up again.

use std::fmt;

use crate::schema::{Check, Column, EnumType, ForeignKey, Index, OtherObject, Schema, Table, View};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    CreateTable {
        table: Table,
    },
    DropTable {
        table: Table,
    },
    RenameTable {
        from: String,
        to: String,
    },

    AddColumn {
        table: String,
        column: Column,
        position: usize,
    },
    DropColumn {
        table: String,
        column: Column,
        position: usize,
    },
    RenameColumn {
        table: String,
        from: String,
        to: String,
    },
    /// Any change to one column's definition (type, nullability, default, … and, for MySQL
    /// `CHANGE COLUMN`, its name).
    AlterColumn {
        table: String,
        before: Column,
        after: Column,
    },

    SetPrimaryKey {
        table: String,
        before: Vec<String>,
        after: Vec<String>,
    },
    AddIndex {
        table: String,
        index: Index,
    },
    DropIndex {
        table: String,
        index: Index,
    },
    AddForeignKey {
        table: String,
        fk: ForeignKey,
    },
    DropForeignKey {
        table: String,
        fk: ForeignKey,
    },
    AddCheck {
        table: String,
        check: Check,
    },
    DropCheck {
        table: String,
        check: Check,
    },
    RenameConstraint {
        table: String,
        from: String,
        to: String,
    },

    /// Table options (MySQL `ENGINE`, `DEFAULT CHARSET`, …). `convert` = `CONVERT TO CHARACTER SET`.
    SetTableOptions {
        table: String,
        before: std::collections::BTreeMap<String, String>,
        after: std::collections::BTreeMap<String, String>,
        convert: bool,
    },
    SetRowLevelSecurity {
        table: String,
        enabled: bool,
    },
    SetComment {
        table: String,
        column: Option<String>,
        before: Option<String>,
        after: Option<String>,
    },

    CreateView {
        view: View,
        replaced: Option<View>,
    },
    DropView {
        view: View,
    },

    CreateEnum {
        enum_type: EnumType,
    },
    DropEnum {
        enum_type: EnumType,
    },
    AddEnumValue {
        name: String,
        value: String,
    },

    CreateObject {
        object: OtherObject,
        replaced: bool,
    },
    DropObject {
        object: OtherObject,
    },

    /// Data manipulation (`INSERT`, `UPDATE`, `DELETE`, …): no schema effect, unknown data effect.
    Data {
        sql: String,
    },
    /// Session / connection / transaction control (`SET`, `PRAGMA`, `BEGIN`, …): no lasting effect.
    Session {
        sql: String,
    },
    /// A statement whose effect rustable doesn't know: unparseable, or not modelled yet.
    Unknown {
        sql: String,
        reason: String,
    },
}

impl Change {
    /// Apply this change to `schema`.
    pub fn apply_to(&self, schema: &mut Schema) {
        match self {
            Change::CreateTable { table } => {
                schema.tables.retain(|t| t.name != table.name);
                schema.tables.push(table.clone());
            }
            Change::DropTable { table } => {
                schema.tables.retain(|t| t.name != table.name);
                schema
                    .objects
                    .retain(|o| o.table.as_deref() != Some(table.name.as_str()));
            }
            Change::RenameTable { from, to } => {
                if let Some(t) = schema.table_mut(from) {
                    t.name.clone_from(to);
                }
                for t in &mut schema.tables {
                    for fk in &mut t.foreign_keys {
                        if fk.references_table == *from {
                            fk.references_table.clone_from(to);
                        }
                    }
                }
            }

            Change::AddColumn {
                table,
                column,
                position,
            } => {
                if let Some(t) = schema.table_mut(table) {
                    let at = (*position).min(t.columns.len());
                    t.columns.insert(at, column.clone());
                }
            }
            Change::DropColumn { table, column, .. } => {
                if let Some(t) = schema.table_mut(table) {
                    t.columns.retain(|c| c.name != column.name);
                    t.primary_key.retain(|c| *c != column.name);
                }
            }
            Change::RenameColumn { table, from, to } => rename_column(schema, table, from, to),
            Change::AlterColumn {
                table,
                before,
                after,
            } => {
                if let Some(t) = schema.table_mut(table)
                    && let Some(c) = t.column_mut(&before.name)
                {
                    *c = after.clone();
                }
                if before.name != after.name {
                    rename_column_references(schema, table, &before.name, &after.name);
                }
            }

            Change::SetPrimaryKey { table, after, .. } => {
                if let Some(t) = schema.table_mut(table) {
                    t.primary_key.clone_from(after);
                    for c in &mut t.columns {
                        if after.contains(&c.name) {
                            c.nullable = false;
                        }
                    }
                }
            }
            Change::AddIndex { table, index } => {
                if let Some(t) = schema.table_mut(table) {
                    t.indexes.push(index.clone());
                }
            }
            Change::DropIndex { table, index } => {
                if let Some(t) = schema.table_mut(table) {
                    remove_first(&mut t.indexes, |i| i == index);
                }
            }
            Change::AddForeignKey { table, fk } => {
                if let Some(t) = schema.table_mut(table) {
                    t.foreign_keys.push(fk.clone());
                }
            }
            Change::DropForeignKey { table, fk } => {
                if let Some(t) = schema.table_mut(table) {
                    remove_first(&mut t.foreign_keys, |f| f == fk);
                }
            }
            Change::AddCheck { table, check } => {
                if let Some(t) = schema.table_mut(table) {
                    t.checks.push(check.clone());
                }
            }
            Change::DropCheck { table, check } => {
                if let Some(t) = schema.table_mut(table) {
                    remove_first(&mut t.checks, |c| c == check);
                }
            }
            Change::RenameConstraint { table, from, to } => {
                if let Some(t) = schema.table_mut(table) {
                    let rename = |name: &mut Option<String>| {
                        if name.as_deref() == Some(from.as_str()) {
                            *name = Some(to.clone());
                        }
                    };
                    t.indexes.iter_mut().for_each(|i| rename(&mut i.name));
                    t.foreign_keys.iter_mut().for_each(|f| rename(&mut f.name));
                    t.checks.iter_mut().for_each(|c| rename(&mut c.name));
                }
            }

            Change::SetTableOptions { table, after, .. } => {
                if let Some(t) = schema.table_mut(table) {
                    t.options.clone_from(after);
                }
            }
            Change::SetRowLevelSecurity { table, enabled } => {
                if let Some(t) = schema.table_mut(table) {
                    t.row_level_security = *enabled;
                }
            }
            Change::SetComment {
                table,
                column,
                after,
                ..
            } => {
                if let Some(t) = schema.table_mut(table) {
                    match column {
                        None => t.comment.clone_from(after),
                        Some(col) => {
                            if let Some(c) = t.column_mut(col) {
                                c.comment.clone_from(after);
                            }
                        }
                    }
                }
            }

            Change::CreateView { view, .. } => {
                schema.views.retain(|v| v.name != view.name);
                schema.views.push(view.clone());
            }
            Change::DropView { view } => {
                schema.views.retain(|v| v.name != view.name);
                schema
                    .objects
                    .retain(|o| o.table.as_deref() != Some(view.name.as_str()));
            }

            Change::CreateEnum { enum_type } => {
                schema.enums.retain(|e| e.name != enum_type.name);
                schema.enums.push(enum_type.clone());
            }
            Change::DropEnum { enum_type } => schema.enums.retain(|e| e.name != enum_type.name),
            Change::AddEnumValue { name, value } => {
                if let Some(e) = schema.enums.iter_mut().find(|e| e.name == *name)
                    && !e.values.contains(value)
                {
                    e.values.push(value.clone());
                }
            }

            Change::CreateObject { object, .. } => {
                schema.objects.retain(|o| !same_object(o, object));
                schema.objects.push(object.clone());
            }
            Change::DropObject { object } => schema.objects.retain(|o| !same_object(o, object)),

            Change::Data { .. } | Change::Session { .. } | Change::Unknown { .. } => {}
        }
    }

    /// The change that exactly reverses this one (structurally), if there is one.
    #[must_use]
    pub fn inverse(&self) -> Option<Change> {
        Some(match self.clone() {
            Change::CreateTable { table } => Change::DropTable { table },
            Change::DropTable { table } => Change::CreateTable { table },
            Change::RenameTable { from, to } => Change::RenameTable { from: to, to: from },
            Change::AddColumn {
                table,
                column,
                position,
            } => Change::DropColumn {
                table,
                column,
                position,
            },
            Change::DropColumn {
                table,
                column,
                position,
            } => Change::AddColumn {
                table,
                column,
                position,
            },
            Change::RenameColumn { table, from, to } => Change::RenameColumn {
                table,
                from: to,
                to: from,
            },
            Change::AlterColumn {
                table,
                before,
                after,
            } => Change::AlterColumn {
                table,
                before: after,
                after: before,
            },
            Change::SetPrimaryKey {
                table,
                before,
                after,
            } => Change::SetPrimaryKey {
                table,
                before: after,
                after: before,
            },
            Change::AddIndex { table, index } => Change::DropIndex { table, index },
            Change::DropIndex { table, index } => Change::AddIndex { table, index },
            Change::AddForeignKey { table, fk } => Change::DropForeignKey { table, fk },
            Change::DropForeignKey { table, fk } => Change::AddForeignKey { table, fk },
            Change::AddCheck { table, check } => Change::DropCheck { table, check },
            Change::DropCheck { table, check } => Change::AddCheck { table, check },
            Change::RenameConstraint { table, from, to } => Change::RenameConstraint {
                table,
                from: to,
                to: from,
            },
            Change::SetTableOptions {
                table,
                before,
                after,
                convert,
            } => Change::SetTableOptions {
                table,
                before: after,
                after: before,
                convert,
            },
            Change::SetRowLevelSecurity { table, enabled } => Change::SetRowLevelSecurity {
                table,
                enabled: !enabled,
            },
            Change::SetComment {
                table,
                column,
                before,
                after,
            } => Change::SetComment {
                table,
                column,
                before: after,
                after: before,
            },
            Change::CreateView {
                view,
                replaced: None,
            } => Change::DropView { view },
            Change::CreateView {
                view,
                replaced: Some(old),
            } => Change::CreateView {
                view: old,
                replaced: Some(view),
            },
            Change::DropView { view } => Change::CreateView {
                view,
                replaced: None,
            },
            Change::CreateEnum { enum_type } => Change::DropEnum { enum_type },
            Change::DropEnum { enum_type } => Change::CreateEnum { enum_type },
            Change::CreateObject {
                object,
                replaced: false,
            } => Change::DropObject { object },
            Change::Session { sql } => Change::Session { sql },
            Change::AddEnumValue { .. }
            | Change::CreateObject { replaced: true, .. }
            | Change::DropObject { .. }
            | Change::Data { .. }
            | Change::Unknown { .. } => return None,
        })
    }

    /// True for changes that touch the schema (not data, session, or unknown statements).
    #[must_use]
    pub fn is_schema_change(&self) -> bool {
        !matches!(
            self,
            Change::Data { .. } | Change::Session { .. } | Change::Unknown { .. }
        )
    }
}

fn same_object(a: &OtherObject, b: &OtherObject) -> bool {
    a.kind == b.kind && a.name == b.name
}

fn remove_first<T>(items: &mut Vec<T>, pred: impl Fn(&T) -> bool) {
    if let Some(i) = items.iter().position(pred) {
        items.remove(i);
    }
}

fn rename_column(schema: &mut Schema, table: &str, from: &str, to: &str) {
    if let Some(t) = schema.table_mut(table)
        && let Some(c) = t.column_mut(from)
    {
        c.name = to.to_string();
    }
    rename_column_references(schema, table, from, to);
}

/// After a column rename, fix up everything that refers to the column by name.
fn rename_column_references(schema: &mut Schema, table: &str, from: &str, to: &str) {
    if let Some(t) = schema.table_mut(table) {
        for pk in &mut t.primary_key {
            if pk == from {
                *pk = to.to_string();
            }
        }
        for index in &mut t.indexes {
            for col in &mut index.columns {
                *col = rename_in_index_column(col, from, to);
            }
        }
        for fk in &mut t.foreign_keys {
            for col in &mut fk.columns {
                if col == from {
                    *col = to.to_string();
                }
            }
        }
    }
    for t in &mut schema.tables {
        for fk in &mut t.foreign_keys {
            if fk.references_table == table {
                for col in &mut fk.references_columns {
                    if col == from {
                        *col = to.to_string();
                    }
                }
            }
        }
    }
}

/// Index columns are rendered expressions (`email`, `email(20)`, `created_at DESC`,
/// `lower(email)`); rename a bare column reference inside one.
fn rename_in_index_column(col: &str, from: &str, to: &str) -> String {
    let mut out = String::new();
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        if word.as_str() == from {
            out.push_str(to);
        } else {
            out.push_str(word);
        }
        word.clear();
    };
    for ch in col.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            word.push(ch);
        } else {
            flush(&mut word, &mut out);
            out.push(ch);
        }
    }
    flush(&mut word, &mut out);
    out
}

impl fmt::Display for Change {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Change::CreateTable { table } => {
                write!(
                    f,
                    "create table `{}` ({} columns)",
                    table.name,
                    table.columns.len()
                )
            }
            Change::DropTable { table } => write!(f, "drop table `{}`", table.name),
            Change::RenameTable { from, to } => write!(f, "rename table `{from}` → `{to}`"),
            Change::AddColumn { table, column, .. } => write!(
                f,
                "add column `{table}.{}` {}{}",
                column.name,
                column.data_type,
                if column.nullable {
                    " NULL"
                } else {
                    " NOT NULL"
                }
            ),
            Change::DropColumn { table, column, .. } => {
                write!(f, "drop column `{table}.{}`", column.name)
            }
            Change::RenameColumn { table, from, to } => {
                write!(f, "rename column `{table}.{from}` → `{to}`")
            }
            Change::AlterColumn {
                table,
                before,
                after,
            } => {
                write!(f, "alter column `{table}.{}`: ", before.name)?;
                let mut parts = Vec::new();
                if before.name != after.name {
                    parts.push(format!("rename → `{}`", after.name));
                }
                if before.data_type != after.data_type {
                    parts.push(format!("type {} → {}", before.data_type, after.data_type));
                }
                if before.nullable != after.nullable {
                    parts.push(if after.nullable {
                        "drop NOT NULL".into()
                    } else {
                        "set NOT NULL".into()
                    });
                }
                if before.default != after.default {
                    parts.push(format!(
                        "default {} → {}",
                        before.default.as_deref().unwrap_or("none"),
                        after.default.as_deref().unwrap_or("none")
                    ));
                }
                if parts.is_empty() {
                    parts.push("other attributes".into());
                }
                f.write_str(&parts.join(", "))
            }
            Change::SetPrimaryKey {
                table,
                before,
                after,
            } => write!(
                f,
                "primary key of `{table}`: ({}) → ({})",
                before.join(", "),
                after.join(", ")
            ),
            Change::AddIndex { table, index } => write!(
                f,
                "add {}index {}on `{table}` ({})",
                if index.unique { "unique " } else { "" },
                index
                    .name
                    .as_ref()
                    .map(|n| format!("`{n}` "))
                    .unwrap_or_default(),
                index.columns.join(", ")
            ),
            Change::DropIndex { table, index } => write!(
                f,
                "drop index {}on `{table}`",
                index
                    .name
                    .as_ref()
                    .map(|n| format!("`{n}` "))
                    .unwrap_or_default()
            ),
            Change::AddForeignKey { table, fk } => write!(
                f,
                "add foreign key `{table}` ({}) → `{}` ({})",
                fk.columns.join(", "),
                fk.references_table,
                fk.references_columns.join(", ")
            ),
            Change::DropForeignKey { table, fk } => write!(
                f,
                "drop foreign key {}on `{table}`",
                fk.name
                    .as_ref()
                    .map(|n| format!("`{n}` "))
                    .unwrap_or_default()
            ),
            Change::AddCheck { table, check } => {
                write!(f, "add check on `{table}`: {}", check.expr)
            }
            Change::DropCheck { table, check } => write!(
                f,
                "drop check {}on `{table}`",
                check
                    .name
                    .as_ref()
                    .map(|n| format!("`{n}` "))
                    .unwrap_or_default()
            ),
            Change::RenameConstraint { table, from, to } => {
                write!(f, "rename constraint `{from}` → `{to}` on `{table}`")
            }
            Change::SetTableOptions { table, convert, .. } => {
                if *convert {
                    write!(f, "convert `{table}` to a new character set")
                } else {
                    write!(f, "set table options on `{table}`")
                }
            }
            Change::SetRowLevelSecurity { table, enabled } => write!(
                f,
                "{} row level security on `{table}`",
                if *enabled { "enable" } else { "disable" }
            ),
            Change::SetComment { table, column, .. } => match column {
                Some(c) => write!(f, "set comment on `{table}.{c}`"),
                None => write!(f, "set comment on `{table}`"),
            },
            Change::CreateView { view, replaced } => write!(
                f,
                "{} {}view `{}`",
                if replaced.is_some() {
                    "replace"
                } else {
                    "create"
                },
                if view.materialized {
                    "materialized "
                } else {
                    ""
                },
                view.name
            ),
            Change::DropView { view } => write!(f, "drop view `{}`", view.name),
            Change::CreateEnum { enum_type } => {
                write!(
                    f,
                    "create enum `{}` ({})",
                    enum_type.name,
                    enum_type.values.join(", ")
                )
            }
            Change::DropEnum { enum_type } => write!(f, "drop enum `{}`", enum_type.name),
            Change::AddEnumValue { name, value } => {
                write!(f, "add value '{value}' to enum `{name}`")
            }
            Change::CreateObject { object, replaced } => write!(
                f,
                "{} {} `{}`",
                if *replaced { "replace" } else { "create" },
                object.kind,
                object.name
            ),
            Change::DropObject { object } => write!(f, "drop {} `{}`", object.kind, object.name),
            Change::Data { sql } => write!(f, "data change: {}", summarize(sql)),
            Change::Session { sql } => write!(f, "session statement: {}", summarize(sql)),
            Change::Unknown { sql, reason } => {
                write!(f, "unknown effect ({reason}): {}", summarize(sql))
            }
        }
    }
}

/// The first line of a statement, cut to a readable length.
#[must_use]
pub fn summarize(sql: &str) -> String {
    let line = sql.lines().next().unwrap_or_default().trim();
    if line.chars().count() > 80 {
        format!("{}…", line.chars().take(80).collect::<String>())
    } else if sql.lines().nth(1).is_some() {
        format!("{line} …")
    } else {
        line.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renames_columns_inside_index_expressions() {
        assert_eq!(
            rename_in_index_column("email(20)", "email", "mail"),
            "mail(20)"
        );
        assert_eq!(
            rename_in_index_column("lower(email)", "email", "mail"),
            "lower(mail)"
        );
        assert_eq!(
            rename_in_index_column("email_verified", "email", "mail"),
            "email_verified"
        );
        assert_eq!(
            rename_in_index_column("created_at DESC", "created_at", "at"),
            "at DESC"
        );
    }
}

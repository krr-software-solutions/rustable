//! Classify changes: can they be undone, and are they safe to deploy in either order relative
//! to application code?
//!
//! ## Compatibility, in deploy terms
//!
//! - **Backward compatible**: the app code that ran *before* this migration keeps working
//!   *after* it. Adding a nullable column is backward compatible; dropping or renaming a column
//!   is not (old code still reads it). A backward-compatible migration can run *before* the new
//!   code is deployed: an "expand" step.
//! - **Forward compatible**: the app code written *for after* this migration also works *before*
//!   it. Dropping a column the new code no longer uses is forward compatible; adding a table the
//!   new code needs is not. A forward-compatible migration can run *after* the new code is
//!   deployed: a "contract" step.
//!
//! A migration that is neither needs a coordinated, lock-step deploy (or downtime).
//!
//! ## Reversibility
//!
//! - **Lossless**: undoing it restores the schema and the data.
//! - **Conditional**: undoing it restores the schema, but may fail depending on data written
//!   since (e.g. undoing `DROP NOT NULL` fails if NULLs were inserted).
//! - **Lossy**: the schema can be restored, but data is gone (`DROP COLUMN`, `DROP TABLE`,
//!   narrowing a type).
//! - **Irreversible**: rustable can't know how to undo it (data changes, unknown statements, or
//!   things the database can't undo, like removing a Postgres enum value).

use std::fmt;

use crate::change::Change;
use crate::schema::Column;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Reversibility {
    Lossless,
    Conditional(String),
    Lossy(String),
    Irreversible(String),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Compat {
    Yes,
    /// Probably fine, but depends on what the app actually does.
    Risky(String),
    No(String),
}

/// When a migration can be deployed relative to the code that needs it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeployPhase {
    /// Backward and forward compatible: deploy in any order.
    AnyOrder,
    /// Backward compatible only: run it before deploying the new code.
    Expand,
    /// Forward compatible only: run it after deploying the new code.
    Contract,
    /// Neither: needs a coordinated deploy.
    Breaking,
}

impl Reversibility {
    #[must_use]
    pub fn is_lossless(&self) -> bool {
        matches!(self, Reversibility::Lossless)
    }

    /// The least reversible of a set (`Lossless` for an empty set).
    #[must_use]
    pub fn worst<'a>(items: impl IntoIterator<Item = &'a Reversibility>) -> Reversibility {
        items
            .into_iter()
            .max()
            .cloned()
            .unwrap_or(Reversibility::Lossless)
    }

    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Reversibility::Lossless => "lossless",
            Reversibility::Conditional(_) => "conditional",
            Reversibility::Lossy(_) => "lossy",
            Reversibility::Irreversible(_) => "irreversible",
        }
    }

    #[must_use]
    pub fn reason(&self) -> Option<&str> {
        match self {
            Reversibility::Lossless => None,
            Reversibility::Conditional(r)
            | Reversibility::Lossy(r)
            | Reversibility::Irreversible(r) => Some(r),
        }
    }
}

impl Compat {
    #[must_use]
    pub fn is_yes(&self) -> bool {
        matches!(self, Compat::Yes)
    }

    /// The worst of a set (`Yes` for an empty set).
    #[must_use]
    pub fn worst<'a>(items: impl IntoIterator<Item = &'a Compat>) -> Compat {
        items.into_iter().max().cloned().unwrap_or(Compat::Yes)
    }

    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Compat::Yes => "yes",
            Compat::Risky(_) => "risky",
            Compat::No(_) => "no",
        }
    }

    #[must_use]
    pub fn reason(&self) -> Option<&str> {
        match self {
            Compat::Yes => None,
            Compat::Risky(r) | Compat::No(r) => Some(r),
        }
    }
}

impl DeployPhase {
    /// Risky counts as compatible here: the phase is a recommendation, the risks are reported
    /// separately.
    #[must_use]
    pub fn from_compat(backward: &Compat, forward: &Compat) -> DeployPhase {
        match (
            matches!(backward, Compat::No(_)),
            matches!(forward, Compat::No(_)),
        ) {
            (false, false) => DeployPhase::AnyOrder,
            (false, true) => DeployPhase::Expand,
            (true, false) => DeployPhase::Contract,
            (true, true) => DeployPhase::Breaking,
        }
    }
}

impl fmt::Display for DeployPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            DeployPhase::AnyOrder => "any order",
            DeployPhase::Expand => "expand (run before deploying code)",
            DeployPhase::Contract => "contract (run after deploying code)",
            DeployPhase::Breaking => "breaking (needs a coordinated deploy)",
        })
    }
}

/// How reversible one change is.
#[must_use]
pub fn reversibility(change: &Change) -> Reversibility {
    use Reversibility::{Conditional, Irreversible, Lossless, Lossy};
    match change {
        Change::CreateTable { .. }
        | Change::AddColumn { .. }
        | Change::RenameTable { .. }
        | Change::RenameColumn { .. }
        | Change::AddIndex { .. }
        | Change::DropIndex { .. }
        | Change::AddForeignKey { .. }
        | Change::AddCheck { .. }
        | Change::RenameConstraint { .. }
        | Change::SetRowLevelSecurity { .. }
        | Change::SetComment { .. }
        | Change::CreateView { .. }
        | Change::DropView { .. }
        | Change::CreateEnum { .. }
        | Change::CreateObject {
            replaced: false, ..
        }
        | Change::Session { .. } => Lossless,

        Change::SetTableOptions {
            convert,
            before,
            after,
            ..
        } => {
            if *convert || before.get("DEFAULT CHARSET") != after.get("DEFAULT CHARSET") {
                Conditional("re-encoding back to the old character set fails or mangles characters it can't hold".into())
            } else {
                Lossless
            }
        }
        Change::DropTable { table } => Lossy(format!("dropping `{}` deletes its rows", table.name)),
        Change::DropColumn { table, column, .. } => Lossy(format!(
            "dropping `{table}.{}` deletes its values",
            column.name
        )),
        Change::AlterColumn { before, after, .. } => alter_column_reversibility(before, after),
        Change::SetPrimaryKey { .. } => {
            Conditional("restoring the old primary key may fail on duplicates".into())
        }
        Change::DropForeignKey { fk, .. } => Conditional(format!(
            "re-adding the foreign key to `{}` fails if orphaned rows were written",
            fk.references_table
        )),
        Change::DropCheck { .. } => {
            Conditional("re-adding the check fails if violating rows were written".into())
        }
        Change::DropEnum { enum_type } => Conditional(format!(
            "re-creating enum `{}` loses columns that used it",
            enum_type.name
        )),
        Change::AddEnumValue { name, value } => Irreversible(format!(
            "Postgres can't remove value '{value}' from enum `{name}`"
        )),
        Change::CreateObject {
            replaced: true,
            object,
        } => Irreversible(format!(
            "the previous definition of {} `{}` isn't known",
            object.kind, object.name
        )),
        Change::DropObject { object } => Irreversible(format!(
            "the definition of {} `{}` isn't known, so it can't be re-created",
            object.kind, object.name
        )),
        Change::Data { .. } => Irreversible("data changes can't be undone automatically".into()),
        Change::Unknown { reason, .. } => Irreversible(format!("unknown effect: {reason}")),
    }
}

fn alter_column_reversibility(before: &Column, after: &Column) -> Reversibility {
    if before.data_type != after.data_type {
        return match type_change(&before.data_type, &after.data_type) {
            TypeChange::Widening => Reversibility::Conditional(format!(
                "narrowing back to {} fails if wider values were written",
                before.data_type
            )),
            TypeChange::Narrowing | TypeChange::Other => Reversibility::Lossy(format!(
                "converting {} → {} may truncate or alter values",
                before.data_type, after.data_type
            )),
        };
    }
    if before.nullable && !after.nullable {
        // Undo is DROP NOT NULL: always works.
        return Reversibility::Lossless;
    }
    if !before.nullable && after.nullable {
        return Reversibility::Conditional("restoring NOT NULL fails if NULLs were written".into());
    }
    Reversibility::Lossless
}

/// Is the app code from before this change still fine after it?
#[must_use]
pub fn backward_compat(change: &Change) -> Compat {
    use Compat::{No, Risky, Yes};
    match change {
        Change::CreateTable { .. }
        | Change::AddIndex {
            index: crate::schema::Index { unique: false, .. },
            ..
        }
        | Change::DropIndex { .. }
        | Change::DropForeignKey { .. }
        | Change::DropCheck { .. }
        | Change::RenameConstraint { .. }
        | Change::SetComment { .. }
        | Change::CreateView { replaced: None, .. }
        | Change::CreateEnum { .. }
        | Change::AddEnumValue { .. }
        | Change::CreateObject {
            replaced: false, ..
        }
        | Change::Session { .. } => Yes,

        Change::AddColumn { column, table, .. } => {
            if column.nullable
                || column.default.is_some()
                || column.auto_increment
                || column.generated.is_some()
            {
                Yes
            } else {
                No(format!(
                    "`{table}.{}` is NOT NULL without a default: old INSERTs that don't set it fail",
                    column.name
                ))
            }
        }
        Change::DropTable { table } => No(format!("old code may still use table `{}`", table.name)),
        Change::DropColumn { table, column, .. } => No(format!(
            "old code may still read or write `{table}.{}`",
            column.name
        )),
        Change::RenameTable { from, .. } => No(format!("old code uses the name `{from}`")),
        Change::RenameColumn { table, from, .. } => {
            No(format!("old code uses the name `{table}.{from}`"))
        }
        Change::AlterColumn {
            table,
            before,
            after,
        } => {
            if before.name != after.name {
                return No(format!("old code uses the name `{table}.{}`", before.name));
            }
            if before.data_type != after.data_type
                && type_change(&before.data_type, &after.data_type) != TypeChange::Widening
            {
                return No(format!(
                    "`{table}.{}` changed {} → {}: old code may write values that no longer fit",
                    before.name, before.data_type, after.data_type
                ));
            }
            if before.nullable && !after.nullable {
                return No(format!(
                    "`{table}.{}` became NOT NULL: old code may write NULLs",
                    before.name
                ));
            }
            if before.default.is_some() && after.default.is_none() && !after.nullable {
                return No(format!(
                    "`{table}.{}` lost its default: old INSERTs that relied on it fail",
                    before.name
                ));
            }
            if before.default != after.default {
                return Risky(format!("default of `{table}.{}` changed", before.name));
            }
            Yes
        }
        Change::SetTableOptions { table, convert, .. } => {
            if *convert {
                Risky(format!(
                    "`{table}` changed character set: string comparisons and lengths may differ"
                ))
            } else {
                Yes
            }
        }
        Change::SetPrimaryKey { table, .. } => Risky(format!("primary key of `{table}` changed")),
        Change::AddIndex { table, .. } => Risky(format!(
            "new unique index on `{table}` may reject old writes"
        )),
        Change::AddForeignKey { table, .. } => Risky(format!(
            "new foreign key on `{table}` may reject old writes"
        )),
        Change::AddCheck { table, .. } => {
            Risky(format!("new check on `{table}` may reject old writes"))
        }
        Change::SetRowLevelSecurity { table, enabled } => {
            if *enabled {
                Risky(format!(
                    "row level security on `{table}` may hide rows from old code"
                ))
            } else {
                Yes
            }
        }
        Change::CreateView {
            view,
            replaced: Some(_),
        } => Risky(format!("view `{}` changed shape", view.name)),
        Change::DropView { view } => No(format!("old code may still use view `{}`", view.name)),
        Change::DropEnum { enum_type } => {
            No(format!("old code may still use enum `{}`", enum_type.name))
        }
        Change::CreateObject {
            replaced: true,
            object,
        } => Risky(format!("{} `{}` was redefined", object.kind, object.name)),
        Change::DropObject { object } => Risky(format!(
            "old code may rely on {} `{}`",
            object.kind, object.name
        )),
        Change::Data { .. } => Risky("data was changed".into()),
        Change::Unknown { reason, .. } => Risky(format!("unknown effect: {reason}")),
    }
}

/// Would the app code written for after this change also work before it?
#[must_use]
pub fn forward_compat(change: &Change) -> Compat {
    use Compat::{No, Risky, Yes};
    match change {
        Change::DropTable { .. }
        | Change::DropColumn { .. }
        | Change::DropIndex { .. }
        | Change::AddForeignKey { .. }
        | Change::AddCheck { .. }
        | Change::RenameConstraint { .. }
        | Change::SetComment { .. }
        | Change::DropView { .. }
        | Change::DropEnum { .. }
        | Change::DropObject { .. }
        | Change::SetTableOptions { .. }
        | Change::Session { .. } => Yes,

        Change::AddIndex { index, table } => {
            if index.unique {
                Risky(format!("new code may rely on uniqueness on `{table}`"))
            } else {
                Yes
            }
        }
        Change::CreateTable { table } => No(format!("new code needs table `{}`", table.name)),
        Change::AddColumn { table, column, .. } => {
            No(format!("new code may use `{table}.{}`", column.name))
        }
        Change::RenameTable { to, .. } => No(format!("new code uses the name `{to}`")),
        Change::RenameColumn { table, to, .. } => {
            No(format!("new code uses the name `{table}.{to}`"))
        }
        Change::AlterColumn {
            table,
            before,
            after,
        } => {
            if before.name != after.name {
                return No(format!("new code uses the name `{table}.{}`", after.name));
            }
            if before.data_type != after.data_type
                && type_change(&before.data_type, &after.data_type) != TypeChange::Narrowing
            {
                return No(format!(
                    "`{table}.{}` changed {} → {}: new code may write values the old type can't hold",
                    before.name, before.data_type, after.data_type
                ));
            }
            if !before.nullable && after.nullable {
                return No(format!(
                    "`{table}.{}` became nullable: new code may write NULLs",
                    before.name
                ));
            }
            if before.default.is_none() && after.default.is_some() && !after.nullable {
                return No(format!(
                    "`{table}.{}` gained a default: new INSERTs may rely on it",
                    before.name
                ));
            }
            Yes
        }
        Change::SetPrimaryKey { table, .. } => Risky(format!("primary key of `{table}` changed")),
        Change::DropForeignKey { table, .. } => Risky(format!(
            "new code may write rows the old foreign key on `{table}` rejects"
        )),
        Change::DropCheck { table, .. } => Risky(format!(
            "new code may write rows the old check on `{table}` rejects"
        )),
        Change::SetRowLevelSecurity { table, enabled } => {
            if *enabled {
                Yes
            } else {
                Risky(format!("new code may expect to see all rows of `{table}`"))
            }
        }
        Change::CreateView { view, .. } => No(format!("new code may use view `{}`", view.name)),
        Change::CreateEnum { enum_type } => {
            No(format!("new code may use enum `{}`", enum_type.name))
        }
        Change::AddEnumValue { name, value } => {
            No(format!("new code may write '{value}' to enum `{name}`"))
        }
        Change::CreateObject { object, .. } => No(format!(
            "new code may rely on {} `{}`",
            object.kind, object.name
        )),
        Change::Data { .. } => Risky("new code may rely on the data change".into()),
        Change::Unknown { reason, .. } => Risky(format!("unknown effect: {reason}")),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TypeChange {
    Widening,
    Narrowing,
    Other,
}

/// A rough read on whether a type change widens or narrows. Only recognises the common cases:
/// integer sizes, `varchar(n)`/`char(n)` lengths, `numeric(p, s)` precision, and `→ text`.
fn type_change(from: &str, to: &str) -> TypeChange {
    let from_n = from.to_ascii_lowercase();
    let to_n = to.to_ascii_lowercase();
    let int_rank = |t: &str| match t.trim_end_matches(" unsigned") {
        "tinyint" => Some(1),
        "smallint" | "int2" => Some(2),
        "mediumint" => Some(3),
        "int" | "integer" | "int4" => Some(4),
        "bigint" | "int8" => Some(5),
        _ => None,
    };
    if let (Some(a), Some(b)) = (int_rank(&from_n), int_rank(&to_n)) {
        return match a.cmp(&b) {
            std::cmp::Ordering::Less => TypeChange::Widening,
            std::cmp::Ordering::Greater => TypeChange::Narrowing,
            std::cmp::Ordering::Equal => TypeChange::Other,
        };
    }
    let text_like = |t: &str| matches!(t, "text" | "mediumtext" | "longtext");
    let sized = |t: &str| -> Option<(String, Vec<u32>)> {
        let (base, rest) = t.split_once('(')?;
        let args = rest
            .trim_end_matches(')')
            .split(',')
            .map(|a| a.trim().parse().ok())
            .collect::<Option<Vec<u32>>>()?;
        Some((base.trim().to_string(), args))
    };
    match (sized(&from_n), sized(&to_n)) {
        (Some((a, x)), Some((b, y))) if a == b || (is_char(&a) && is_char(&b)) => {
            if y.iter().zip(&x).all(|(new, old)| new >= old) {
                TypeChange::Widening
            } else if y.iter().zip(&x).all(|(new, old)| new <= old) {
                TypeChange::Narrowing
            } else {
                TypeChange::Other
            }
        }
        (Some((a, _)), None) if is_char(&a) && text_like(&to_n) => TypeChange::Widening,
        (None, Some((b, _))) if is_char(&b) && text_like(&from_n) => TypeChange::Narrowing,
        _ if from_n == "text" && text_like(&to_n) => TypeChange::Widening,
        _ => TypeChange::Other,
    }
}

fn is_char(base: &str) -> bool {
    matches!(
        base,
        "varchar" | "character varying" | "char" | "character" | "nvarchar" | "varbinary"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_type_changes() {
        assert_eq!(
            type_change("VARCHAR(255)", "VARCHAR(64)"),
            TypeChange::Narrowing
        );
        assert_eq!(type_change("VARCHAR(64)", "TEXT"), TypeChange::Widening);
        assert_eq!(type_change("INT", "BIGINT"), TypeChange::Widening);
        assert_eq!(type_change("INTEGER", "SMALLINT"), TypeChange::Narrowing);
        assert_eq!(type_change("TEXT", "VARCHAR(120)"), TypeChange::Narrowing);
        assert_eq!(type_change("BIGINT", "NUMERIC(12,2)"), TypeChange::Other);
    }

    #[test]
    fn deploy_phase() {
        let no = Compat::No(String::new());
        assert_eq!(
            DeployPhase::from_compat(&Compat::Yes, &no),
            DeployPhase::Expand
        );
        assert_eq!(
            DeployPhase::from_compat(&no, &Compat::Yes),
            DeployPhase::Contract
        );
        assert_eq!(DeployPhase::from_compat(&no, &no), DeployPhase::Breaking);
    }
}

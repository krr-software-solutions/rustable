//! Turning `sqlparser` identifiers into the names rustable stores.
//!
//! Unquoted identifiers are case-insensitive in all three dialects, so they're lowercased;
//! quoted identifiers keep their exact spelling. For Postgres, the default `public.` schema
//! prefix is dropped so `public.users` and `users` are the same table.

use sqlparser::ast::{Ident, ObjectName, ObjectNamePart};

use crate::dialect::Dialect;

#[must_use]
pub fn ident(i: &Ident) -> String {
    if i.quote_style.is_some() {
        i.value.clone()
    } else {
        i.value.to_lowercase()
    }
}

#[must_use]
pub fn object_name(name: &ObjectName, dialect: Dialect) -> String {
    let parts: Vec<String> = name
        .0
        .iter()
        .map(|part| match part {
            ObjectNamePart::Identifier(i) => ident(i),
            other => other.to_string(),
        })
        .collect();
    let joined = parts.join(".");
    match dialect {
        Dialect::Postgres => joined
            .strip_prefix("public.")
            .map_or(joined.clone(), str::to_string),
        Dialect::Sqlite => joined
            .strip_prefix("main.")
            .map_or(joined.clone(), str::to_string),
        Dialect::MySql => joined,
    }
}

/// The last part of a possibly-qualified name (`billing.invoices` → `invoices`).
#[must_use]
pub fn unqualified(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}

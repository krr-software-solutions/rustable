//! Compare two schemas and describe the differences.

use std::collections::BTreeSet;
use std::fmt::Display;

use crate::schema::{Column, Schema, Table};

/// Human-readable differences going from `from` to `to` (empty if they're the same).
///
/// Column order is ignored: it rarely matters, and SQLite/Postgres can't reorder columns anyway.
#[must_use]
pub fn diff(from: &Schema, to: &Schema) -> Vec<String> {
    let mut out = Vec::new();

    let names = |s: &Schema| {
        s.tables
            .iter()
            .map(|t| t.name.clone())
            .collect::<BTreeSet<_>>()
    };
    let (a, b) = (names(from), names(to));
    for name in a.difference(&b) {
        out.push(format!("table `{name}` removed"));
    }
    for name in b.difference(&a) {
        out.push(format!("table `{name}` added"));
    }
    for name in a.intersection(&b) {
        if let (Some(x), Some(y)) = (from.table(name), to.table(name)) {
            diff_table(x, y, &mut out);
        }
    }

    diff_named(&from.views, &to.views, |v| v.name.clone(), "view", &mut out);
    diff_named(&from.enums, &to.enums, |e| e.name.clone(), "enum", &mut out);
    diff_named(
        &from.objects,
        &to.objects,
        |o| format!("{} {}", o.kind, o.name),
        "object",
        &mut out,
    );
    out
}

fn diff_table(a: &Table, b: &Table, out: &mut Vec<String>) {
    let t = &a.name;
    let names = |t: &Table| {
        t.columns
            .iter()
            .map(|c| c.name.clone())
            .collect::<BTreeSet<_>>()
    };
    let (x, y) = (names(a), names(b));
    for c in x.difference(&y) {
        out.push(format!("column `{t}.{c}` removed"));
    }
    for c in y.difference(&x) {
        out.push(format!("column `{t}.{c}` added"));
    }
    for c in x.intersection(&y) {
        if let (Some(p), Some(q)) = (a.column(c), b.column(c)) {
            diff_column(t, p, q, out);
        }
    }
    if a.primary_key != b.primary_key {
        out.push(format!(
            "primary key of `{t}`: ({}) → ({})",
            a.primary_key.join(", "),
            b.primary_key.join(", ")
        ));
    }
    diff_set(&a.indexes, &b.indexes, &format!("index on `{t}`"), out);
    diff_set(
        &a.foreign_keys,
        &b.foreign_keys,
        &format!("foreign key on `{t}`"),
        out,
    );
    diff_set(&a.checks, &b.checks, &format!("check on `{t}`"), out);
    if a.options != b.options {
        out.push(format!(
            "table options of `{t}`: {:?} → {:?}",
            a.options, b.options
        ));
    }
    if a.comment != b.comment {
        out.push(format!("comment on `{t}` changed"));
    }
    if a.row_level_security != b.row_level_security {
        out.push(format!("row level security on `{t}` changed"));
    }
}

fn diff_column(t: &str, a: &Column, b: &Column, out: &mut Vec<String>) {
    let c = &a.name;
    if a.data_type != b.data_type {
        out.push(format!(
            "column `{t}.{c}` type: {} → {}",
            a.data_type, b.data_type
        ));
    }
    if a.nullable != b.nullable {
        out.push(format!(
            "column `{t}.{c}`: {}",
            if b.nullable {
                "became nullable"
            } else {
                "became NOT NULL"
            }
        ));
    }
    if a.default != b.default {
        out.push(format!(
            "column `{t}.{c}` default: {} → {}",
            a.default.as_deref().unwrap_or("none"),
            b.default.as_deref().unwrap_or("none")
        ));
    }
    let rest = |col: &Column| {
        (
            col.auto_increment,
            col.generated.clone(),
            col.charset.clone(),
            col.collation.clone(),
            col.on_update.clone(),
            col.comment.clone(),
        )
    };
    if rest(a) != rest(b) {
        out.push(format!("column `{t}.{c}`: other attributes changed"));
    }
}

/// Compare two lists as multisets; report what's only on one side.
fn diff_set<T: PartialEq + Display>(a: &[T], b: &[T], what: &str, out: &mut Vec<String>) {
    let mut remaining: Vec<&T> = b.iter().collect();
    for item in a {
        if let Some(i) = remaining.iter().position(|x| *x == item) {
            remaining.remove(i);
        } else {
            out.push(format!("{what} removed: {item}"));
        }
    }
    for item in remaining {
        out.push(format!("{what} added: {item}"));
    }
}

fn diff_named<T: PartialEq>(
    a: &[T],
    b: &[T],
    key: impl Fn(&T) -> String,
    what: &str,
    out: &mut Vec<String>,
) {
    for x in a {
        match b.iter().find(|y| key(y) == key(x)) {
            None => out.push(format!("{what} `{}` removed", key(x))),
            Some(y) if y != x => out.push(format!("{what} `{}` changed", key(x))),
            Some(_) => {}
        }
    }
    for y in b {
        if !a.iter().any(|x| key(x) == key(y)) {
            out.push(format!("{what} `{}` added", key(y)));
        }
    }
}

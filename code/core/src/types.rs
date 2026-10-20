//! Canonical spellings for SQL types, so `int`, `INT4` and `INTEGER` compare equal.
//!
//! Types are uppercased (except inside quotes, e.g. MySQL `ENUM('a','b')` values) and common
//! aliases are mapped to one spelling per dialect. Both the migrations side and the code side go
//! through this, so a diff between them only shows real differences.

use crate::dialect::Dialect;

#[must_use]
pub fn canonical_type(raw: &str, dialect: Dialect) -> String {
    let upper = upper_outside_quotes(raw.trim());
    let (base, rest) = match upper.find(['(', '[']) {
        Some(i) => (upper[..i].trim_end().to_string(), upper[i..].to_string()),
        None => (upper.clone(), String::new()),
    };
    let base = collapse_spaces(&base);
    let mapped = match dialect {
        Dialect::Postgres => match base.as_str() {
            "INT" | "INT4" => "INTEGER",
            "INT8" => "BIGINT",
            "INT2" => "SMALLINT",
            "BOOL" => "BOOLEAN",
            "FLOAT8" | "DOUBLE" => "DOUBLE PRECISION",
            "FLOAT4" => "REAL",
            "DECIMAL" => "NUMERIC",
            "CHARACTER VARYING" => "VARCHAR",
            "CHARACTER" => "CHAR",
            "TIMESTAMP WITH TIME ZONE" => "TIMESTAMPTZ",
            "TIMESTAMP WITHOUT TIME ZONE" => "TIMESTAMP",
            "SERIAL4" => "SERIAL",
            "SERIAL8" => "BIGSERIAL",
            other => other,
        },
        Dialect::MySql => match base.as_str() {
            "INTEGER" => "INT",
            "BOOL" | "BOOLEAN" => return "TINYINT(1)".to_string(),
            "DEC" | "NUMERIC" => "DECIMAL",
            other => other,
        },
        Dialect::Sqlite => base.as_str(),
    };
    format!("{mapped}{rest}")
}

fn upper_outside_quotes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut quote: Option<char> = None;
    for ch in s.chars() {
        match quote {
            Some(q) if ch == q => {
                quote = None;
                out.push(ch);
            }
            Some(_) => out.push(ch),
            None if ch == '\'' || ch == '"' || ch == '`' => {
                quote = Some(ch);
                out.push(ch);
            }
            None => out.push(ch.to_ascii_uppercase()),
        }
    }
    out
}

fn collapse_spaces(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalizes() {
        assert_eq!(canonical_type("int", Dialect::Postgres), "INTEGER");
        assert_eq!(canonical_type("bigserial", Dialect::Postgres), "BIGSERIAL");
        assert_eq!(
            canonical_type("character varying(20)", Dialect::Postgres),
            "VARCHAR(20)"
        );
        assert_eq!(
            canonical_type("timestamp with time zone", Dialect::Postgres),
            "TIMESTAMPTZ"
        );
        assert_eq!(canonical_type("text[]", Dialect::Postgres), "TEXT[]");
        assert_eq!(
            canonical_type("enum('a','B')", Dialect::MySql),
            "ENUM('a','B')"
        );
        assert_eq!(
            canonical_type("integer unsigned", Dialect::MySql),
            "INTEGER UNSIGNED"
        );
        assert_eq!(canonical_type("boolean", Dialect::MySql), "TINYINT(1)");
    }
}

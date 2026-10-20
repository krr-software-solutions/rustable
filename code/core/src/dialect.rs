use serde::{Deserialize, Serialize};

/// The SQL dialects rustable supports.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dialect {
    Postgres,
    MySql,
    Sqlite,
}

impl Dialect {
    #[must_use]
    pub fn parser_dialect(self) -> &'static dyn sqlparser::dialect::Dialect {
        match self {
            Dialect::Postgres => &sqlparser::dialect::PostgreSqlDialect {},
            Dialect::MySql => &sqlparser::dialect::MySqlDialect {},
            Dialect::Sqlite => &sqlparser::dialect::SQLiteDialect {},
        }
    }

    /// Quote an identifier for this dialect, only when it needs quoting.
    #[must_use]
    pub fn quote(self, ident: &str) -> String {
        let simple = !ident.is_empty()
            && ident
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_lowercase() || c == '_')
            && ident
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.');
        if simple && !is_reserved(ident) {
            ident.to_string()
        } else {
            let q = match self {
                Dialect::MySql => '`',
                Dialect::Postgres | Dialect::Sqlite => '"',
            };
            // qualified names: quote each part
            ident
                .split('.')
                .map(|part| format!("{q}{part}{q}"))
                .collect::<Vec<_>>()
                .join(".")
        }
    }
}

fn is_reserved(ident: &str) -> bool {
    matches!(
        ident,
        "user"
            | "order"
            | "group"
            | "select"
            | "table"
            | "index"
            | "key"
            | "default"
            | "check"
            | "references"
            | "primary"
            | "from"
            | "where"
            | "to"
            | "column"
            | "values"
            | "desc"
            | "limit"
            | "offset"
            | "end"
            | "case"
            | "when"
            | "status"
    )
}

impl std::fmt::Display for Dialect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Dialect::Postgres => "postgres",
            Dialect::MySql => "mysql",
            Dialect::Sqlite => "sqlite",
        })
    }
}

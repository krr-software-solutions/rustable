//! The schema model: what rustable knows about a database after folding over its migrations.
//!
//! It serializes to TOML as `schema.toml`. Everything is kept in a deterministic order (tables
//! by name, columns in definition order) so the file diffs cleanly in code review.
//!
//! Types, defaults and expressions are kept as rendered SQL strings rather than modelled
//! structurally: rustable cares whether they *changed*, not what they mean.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Schema {
    #[serde(default, rename = "table", skip_serializing_if = "Vec::is_empty")]
    pub tables: Vec<Table>,
    #[serde(default, rename = "view", skip_serializing_if = "Vec::is_empty")]
    pub views: Vec<View>,
    #[serde(default, rename = "enum", skip_serializing_if = "Vec::is_empty")]
    pub enums: Vec<EnumType>,
    /// Objects rustable tracks by name only: extensions, schemas, functions, triggers, …
    #[serde(default, rename = "object", skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<OtherObject>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Table {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub primary_key: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub row_level_security: bool,
    /// Table options, e.g. MySQL `ENGINE`, `DEFAULT CHARSET`; SQLite `STRICT`, `WITHOUT ROWID`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub options: BTreeMap<String, String>,
    #[serde(default, rename = "column", skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<Column>,
    #[serde(default, rename = "index", skip_serializing_if = "Vec::is_empty")]
    pub indexes: Vec<Index>,
    #[serde(default, rename = "foreign_key", skip_serializing_if = "Vec::is_empty")]
    pub foreign_keys: Vec<ForeignKey>,
    #[serde(default, rename = "check", skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<Check>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Column {
    pub name: String,
    #[serde(rename = "type")]
    pub data_type: String,
    pub nullable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub auto_increment: bool,
    /// `GENERATED … AS IDENTITY` / `GENERATED ALWAYS AS (expr)`, rendered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_update: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Index {
    /// `None` for an index the database named itself (e.g. an inline `UNIQUE`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub columns: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unique: bool,
    /// Declared as a table constraint (`UNIQUE (…)` / `CONSTRAINT x UNIQUE`) rather than an index.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub constraint: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub using: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "where")]
    pub predicate: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForeignKey {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub columns: Vec<String>,
    pub references_table: String,
    pub references_columns: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_delete: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_update: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Check {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub expr: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct View {
    pub name: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub materialized: bool,
    pub query: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumType {
    pub name: String,
    pub values: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct OtherObject {
    pub kind: String,
    pub name: String,
    /// For objects that belong to a table (triggers, policies).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<String>,
}

impl std::fmt::Display for Index {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}{}({})",
            if self.unique { "unique " } else { "" },
            self.name
                .as_ref()
                .map(|n| format!("`{n}` "))
                .unwrap_or_default(),
            self.columns.join(", ")
        )?;
        if let Some(p) = &self.predicate {
            write!(f, " where {p}")?;
        }
        Ok(())
    }
}

impl std::fmt::Display for ForeignKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}({}) → `{}` ({})",
            self.name
                .as_ref()
                .map(|n| format!("`{n}` "))
                .unwrap_or_default(),
            self.columns.join(", "),
            self.references_table,
            self.references_columns.join(", ")
        )
    }
}

impl std::fmt::Display for Check {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}{}",
            self.name
                .as_ref()
                .map(|n| format!("`{n}` "))
                .unwrap_or_default(),
            self.expr
        )
    }
}

impl Schema {
    #[must_use]
    pub fn table(&self, name: &str) -> Option<&Table> {
        self.tables.iter().find(|t| t.name == name)
    }

    pub fn table_mut(&mut self, name: &str) -> Option<&mut Table> {
        self.tables.iter_mut().find(|t| t.name == name)
    }

    /// Find the table that owns an index called `index_name`.
    #[must_use]
    pub fn table_with_index(&self, index_name: &str) -> Option<&Table> {
        self.tables.iter().find(|t| {
            t.indexes
                .iter()
                .any(|i| i.name.as_deref() == Some(index_name))
        })
    }

    /// Keep everything in a stable order, so serialized output is deterministic.
    pub fn normalize(&mut self) {
        self.tables.sort_by(|a, b| a.name.cmp(&b.name));
        self.views.sort_by(|a, b| a.name.cmp(&b.name));
        self.enums.sort_by(|a, b| a.name.cmp(&b.name));
        self.objects.sort();
    }

    /// Render as `schema.toml` contents.
    ///
    /// # Errors
    /// If the TOML serializer rejects the model (it shouldn't).
    pub fn to_toml(&self) -> Result<String, crate::Error> {
        let mut normalized = self.clone();
        normalized.normalize();
        Ok(toml::to_string_pretty(&normalized)?)
    }
}

impl Table {
    #[must_use]
    pub fn new(name: String) -> Table {
        Table {
            name,
            ..Table::default()
        }
    }

    #[must_use]
    pub fn column(&self, name: &str) -> Option<&Column> {
        self.columns.iter().find(|c| c.name == name)
    }

    pub fn column_mut(&mut self, name: &str) -> Option<&mut Column> {
        self.columns.iter_mut().find(|c| c.name == name)
    }

    #[must_use]
    pub fn column_position(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_toml() {
        let mut table = Table::new("users".to_string());
        table.primary_key = vec!["id".to_string()];
        table
            .options
            .insert("ENGINE".to_string(), "InnoDB".to_string());
        table.columns.push(Column {
            name: "id".to_string(),
            data_type: "BIGINT UNSIGNED".to_string(),
            nullable: false,
            auto_increment: true,
            ..Column::default()
        });
        table.indexes.push(Index {
            name: Some("uq_email".to_string()),
            columns: vec!["email".to_string()],
            unique: true,
            ..Index::default()
        });
        let schema = Schema {
            tables: vec![table],
            ..Schema::default()
        };

        let text = schema.to_toml().unwrap();
        assert!(text.contains("[[table]]"), "{text}");
        assert!(text.contains("[[table.column]]"), "{text}");
        let back: Schema = toml::from_str(&text).unwrap();
        assert_eq!(back, schema);
    }
}

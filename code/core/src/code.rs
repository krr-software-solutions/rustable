//! Tables declared in Rust code, via `#[derive(rustable_sql::Table)]`.
//!
//! Every derived table registers itself (through [`inventory`]), so
//! [`registered_schema`] can collect all of them into a [`Schema`]. Write that to
//! `code.current_schema.toml` with [`write_code_schema`], then `rustable diff` / `rustable plan`
//! compare it against the schema your migrations produce.
//!
//! Modelled on Oxygen's `TableRepr` derivation: `snake_case` names by default, `Option<T>` means
//! nullable, and attributes for primary keys, references and indexes, plus column defaults.

use std::path::Path;
use std::time::SystemTime;

pub use inventory;

use crate::dialect::Dialect;
use crate::error::Error;
use crate::schema::{Schema, Table as TableModel};

/// Implemented by `#[derive(Table)]`.
pub trait Table {
    /// This table's definition, for `dialect`.
    fn table(dialect: Dialect) -> TableModel;
}

/// A registered table (submitted by the derive).
pub struct TableDef {
    pub build: fn(Dialect) -> TableModel,
}

inventory::collect!(TableDef);

/// How a Rust type maps to a column type. Implement it for your own newtypes.
pub trait SqlType {
    fn sql_type(dialect: Dialect) -> String;
    /// `Option<T>` is nullable; everything else isn't.
    const NULLABLE: bool = false;
}

impl<T: SqlType> SqlType for Option<T> {
    fn sql_type(dialect: Dialect) -> String {
        T::sql_type(dialect)
    }
    const NULLABLE: bool = true;
}

macro_rules! sql_type {
    ($($t:ty => $pg:expr, $mysql:expr, $sqlite:expr;)*) => {
        $(impl SqlType for $t {
            fn sql_type(dialect: Dialect) -> String {
                match dialect {
                    Dialect::Postgres => $pg,
                    Dialect::MySql => $mysql,
                    Dialect::Sqlite => $sqlite,
                }
                .to_string()
            }
        })*
    };
}

sql_type! {
    i16 => "SMALLINT", "SMALLINT", "INTEGER";
    i32 => "INTEGER", "INT", "INTEGER";
    i64 => "BIGINT", "BIGINT", "INTEGER";
    u8 => "SMALLINT", "TINYINT UNSIGNED", "INTEGER";
    u16 => "INTEGER", "SMALLINT UNSIGNED", "INTEGER";
    u32 => "BIGINT", "INT UNSIGNED", "INTEGER";
    u64 => "NUMERIC(20)", "BIGINT UNSIGNED", "INTEGER";
    f32 => "REAL", "FLOAT", "REAL";
    f64 => "DOUBLE PRECISION", "DOUBLE", "REAL";
    bool => "BOOLEAN", "TINYINT(1)", "INTEGER";
    String => "TEXT", "TEXT", "TEXT";
    Vec<u8> => "BYTEA", "BLOB", "BLOB";
    SystemTime => "TIMESTAMPTZ", "DATETIME", "TEXT";
}

/// Finish a derived table: canonical type spellings and the database's default constraint names.
#[must_use]
pub fn finish(mut table: TableModel, dialect: Dialect) -> TableModel {
    for column in &mut table.columns {
        column.data_type = crate::types::canonical_type(&column.data_type, dialect);
        if table.primary_key.contains(&column.name) {
            column.nullable = false;
        }
    }
    for i in 0..table.indexes.len() {
        if table.indexes[i].name.is_none() && !table.indexes[i].constraint {
            table.indexes[i].name =
                crate::apply::default_index_name(&table, &table.indexes[i], dialect);
        }
    }
    crate::apply::fill_default_names(&mut table, dialect);
    table
}

/// Every table registered by `#[derive(Table)]` in this binary.
#[must_use]
pub fn registered_schema(dialect: Dialect) -> Schema {
    let mut schema = Schema {
        tables: inventory::iter::<TableDef>
            .into_iter()
            .map(|def| (def.build)(dialect))
            .collect(),
        ..Schema::default()
    };
    schema.normalize();
    schema
}

/// Write [`registered_schema`] to `path` (conventionally `code.current_schema.toml`).
///
/// # Errors
/// Serialisation or I/O failures.
pub fn write_code_schema(path: &Path, dialect: Dialect) -> Result<(), Error> {
    let text = registered_schema(dialect).to_toml()?;
    std::fs::write(path, text).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })
}

//! rustable: SQL schema migrations, in the spirit of Flyway.
//!
//! This is the crate to depend on. It re-exports the analysis pipeline from
//! [`rustable-sql-core`](rustable_sql_core) and `#[derive(Table)]` from `rustable-sql-derive`;
//! with the `db` feature it also re-exports [`rustable-sql-db`](rustable_sql_db) as [`db`].
//!
//! ```ignore
//! #[derive(rustable_sql::Table)]
//! #[rustable(table = "bookmarks")]
//! struct Bookmark {
//!     #[rustable(primary_key, sql_type = "BIGSERIAL")]
//!     id: i64,
//!     url: String,
//! }
//! ```

pub use rustable_sql_core::*;
pub use rustable_sql_derive::Table;

#[cfg(feature = "db")]
pub use rustable_sql_db as db;

/// What `#[derive(Table)]` expands to refers to; not part of the public API.
#[doc(hidden)]
pub mod __private {
    pub use rustable_sql_core::*;
}

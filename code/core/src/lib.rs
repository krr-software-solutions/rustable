//! Core library for rustable: migration parsing, schema derivation, and execution.
//!
//! The pipeline, in order:
//!
//! 1. [`discover`] finds `V…__name.sql` / `V…__name.undo.sql` files ([`version`]).
//! 2. [`statements`] splits each file into statements and parses them with `sqlparser`.
//! 3. [`apply`] folds statements into a [`schema::Schema`], recording a [`change::Change`] per effect.
//! 4. [`analysis`] classifies each change: reversibility and forwards/backwards compatibility.
//! 5. [`undo`] renders inverse SQL for changes whose inverse is mechanical.
//! 6. [`project`] ties it together: analyze a whole migrations directory.
//! 7. [`report`] renders `MIGRATIONS.md`; [`diff`] compares schemas.

#![allow(
    clippy::too_many_lines,
    reason = "the fold and the analyses are one big match over statement/change kinds; splitting them up scatters what belongs together"
)]
#![allow(
    clippy::format_push_string,
    reason = "the SQL/markdown renderers build strings piece by piece"
)]
#![allow(
    clippy::struct_excessive_bools,
    reason = "Column/Index flags mirror SQL attributes one to one"
)]
#![allow(
    clippy::result_large_err,
    reason = "errors carry the io/toml error that caused them; they're rare"
)]
#![allow(
    clippy::match_same_arms,
    clippy::many_single_char_names,
    clippy::explicit_counter_loop
)]
#![allow(
    clippy::match_wildcard_for_single_variants,
    clippy::case_sensitive_file_extension_comparisons
)]

pub mod analysis;
pub mod apply;
pub mod change;
pub mod code;
pub mod dialect;
pub mod diff;
pub mod discover;
pub mod error;
pub mod heuristics;
pub mod names;
pub mod plan;
pub mod project;
pub mod render;
pub mod report;
pub mod schema;
pub mod statements;
pub mod types;
pub mod undo;
pub mod version;

pub use dialect::Dialect;
pub use error::Error;

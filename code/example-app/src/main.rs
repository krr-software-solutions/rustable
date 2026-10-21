//! A tiny app whose tables mirror `examples/postgres/ex-1` ("Pinboard"), declared in Rust.
//!
//! It has drifted from the migrations on purpose, the way real code does:
//! - `bookmarks.pinned` is new in the code;
//! - `bookmarks.label` was removed from the code (a soft drop makes it nullable instead).
//!
//! Run it to write `code.current_schema.toml`, then let rustable compare:
//!
//! ```sh
//! cargo run -p rustable-sql-example-app -- examples/postgres/ex-1/code.current_schema.toml
//! rustable diff --config-path examples/postgres/ex-1/config.toml
//! rustable plan --config-path examples/postgres/ex-1/config.toml --soft-drops
//! ```

use std::time::SystemTime;

use rustable_sql::{Dialect, Table};

#[derive(Table)]
#[rustable(table = "users", unique_index = "users_email_lower_key: lower(email)")]
#[allow(dead_code)]
struct User {
    #[rustable(primary_key, sql_type = "BIGSERIAL")]
    id: i64,
    email: String,
    display_name: Option<String>,
    #[rustable(default = "now()")]
    created_at: SystemTime,
}

#[derive(Table)]
#[rustable(
    table = "bookmarks",
    index = "bookmarks_tags_gin_idx: tags using GIN",
    index = "bookmarks_live_feed_idx: user_id, created_at DESC where archived_at IS NULL",
    check = "bookmarks_url_check: url ~* '^https?://'"
)]
#[allow(dead_code)]
struct Bookmark {
    #[rustable(primary_key, sql_type = "BIGSERIAL")]
    id: i64,
    #[rustable(indexed, references = "users(id)", on_delete = "CASCADE")]
    user_id: i64,
    url: String,
    #[rustable(sql_type = "TEXT[]", default = "'{}'")]
    tags: Vec<String>,
    #[rustable(default = "now()")]
    created_at: SystemTime,
    archived_at: Option<SystemTime>,
    #[rustable(default = "false")]
    pinned: bool,
}

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "code.current_schema.toml".to_string());
    match rustable_sql::code::write_code_schema(path.as_ref(), Dialect::Postgres) {
        Ok(()) => println!("wrote {path}"),
        Err(e) => {
            eprintln!("ERROR - {e}");
            std::process::exit(1);
        }
    }
}

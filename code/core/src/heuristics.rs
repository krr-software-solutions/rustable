//! Fallbacks for statements `sqlparser` can't parse (or parses but rustable doesn't model).
//!
//! Real migrations are full of dialect corners: MySQL procedures inside `DELIMITER` blocks,
//! option-only `ALTER TABLE`s, SQLite virtual tables, Postgres `WITH NO DATA`… Rather than give
//! up on these as "unknown", we try, in order:
//!
//! 1. **Repairs**: rewrite a known-unsupported clause away, re-parse, and patch the result.
//! 2. **Word-level recognition** of `CREATE`/`DROP` for procedures, functions, triggers, events
//!    and virtual tables, and of MySQL table-option `ALTER`s.
//!
//! Anything still unrecognised falls back to keyword classification in [`crate::apply`].

use sqlparser::parser::Parser;
use sqlparser::tokenizer::{Token, Tokenizer};

use crate::apply::changes_for;
use crate::change::Change;
use crate::dialect::Dialect;
use crate::schema::{OtherObject, Schema, Table};

/// Try to make sense of an unparsed statement. `None` = no idea.
pub(crate) fn unparsed(schema: &Schema, sql: &str, dialect: Dialect) -> Option<Vec<Change>> {
    repair(schema, sql, dialect).or_else(|| by_words(schema, sql, dialect))
}

/// Re-parse after rewriting clauses `sqlparser` doesn't support.
fn repair(schema: &Schema, sql: &str, dialect: Dialect) -> Option<Vec<Change>> {
    let upper = sql.to_ascii_uppercase();
    let trimmed = upper.trim_end();

    // Postgres: CREATE MATERIALIZED VIEW … WITH [NO] DATA
    if dialect == Dialect::Postgres {
        for suffix in ["WITH NO DATA", "WITH DATA"] {
            if trimmed.ends_with(suffix) {
                let cut = sql.trim_end().len() - suffix.len();
                return reparse(schema, &sql[..cut], dialect);
            }
        }
    }

    // SQLite: `STRICT, WITHOUT ROWID` (both table options together)
    if dialect == Dialect::Sqlite {
        for combo in ["STRICT, WITHOUT ROWID", "WITHOUT ROWID, STRICT"] {
            if let Some(pos) = upper.rfind(combo) {
                let repaired = format!("{}STRICT{}", &sql[..pos], &sql[pos + combo.len()..]);
                let mut changes = reparse(schema, &repaired, dialect)?;
                for change in &mut changes {
                    if let Change::CreateTable { table } = change {
                        table.options.insert("WITHOUT ROWID".into(), "true".into());
                    }
                }
                return Some(changes);
            }
        }
    }
    None
}

fn reparse(schema: &Schema, sql: &str, dialect: Dialect) -> Option<Vec<Change>> {
    let mut parsed = Parser::parse_sql(dialect.parser_dialect(), sql).ok()?;
    (parsed.len() == 1).then(|| changes_for(schema, &parsed.remove(0), dialect, sql))
}

/// A significant token: a word (uppercased, with its original spelling) or other punctuation.
#[derive(Clone, Debug)]
struct Tok {
    /// The token as written (for re-assembling SQL).
    text: String,
    upper: String,
    original: String,
    is_word: bool,
}

fn tokens(sql: &str, dialect: Dialect) -> Option<Vec<Tok>> {
    let raw = Tokenizer::new(dialect.parser_dialect(), sql)
        .tokenize()
        .ok()?;
    Some(
        raw.into_iter()
            .filter(|t| !matches!(t, Token::Whitespace(_)))
            .map(|t| match &t {
                Token::Word(w) => Tok {
                    text: t.to_string(),
                    upper: w.value.to_ascii_uppercase(),
                    original: if w.quote_style.is_some() {
                        w.value.clone()
                    } else {
                        w.value.to_lowercase()
                    },
                    is_word: true,
                },
                Token::SingleQuotedString(s) => Tok {
                    text: t.to_string(),
                    upper: s.to_ascii_uppercase(),
                    original: s.clone(),
                    is_word: false,
                },
                other => Tok {
                    text: other.to_string(),
                    upper: other.to_string(),
                    original: other.to_string(),
                    is_word: false,
                },
            })
            .collect(),
    )
}

/// Read a possibly-qualified name starting at `toks[i]`; returns it and the index after it.
fn read_name(toks: &[Tok], mut i: usize, dialect: Dialect) -> Option<(String, usize)> {
    let mut parts = Vec::new();
    loop {
        let t = toks.get(i)?;
        if !t.is_word {
            return None;
        }
        parts.push(t.original.clone());
        i += 1;
        if toks.get(i).is_some_and(|t| t.upper == ".") {
            i += 1;
        } else {
            break;
        }
    }
    let name = parts.join(".");
    let name = match dialect {
        Dialect::Postgres => name
            .strip_prefix("public.")
            .map_or(name.clone(), str::to_string),
        _ => name,
    };
    Some((name, i))
}

/// Recognise statements from their words alone.
pub(crate) fn by_words(schema: &Schema, sql: &str, dialect: Dialect) -> Option<Vec<Change>> {
    let toks = tokens(sql, dialect)?;
    let words: Vec<&str> = toks.iter().map(|t| t.upper.as_str()).collect();
    let has = |seq: &[&str]| words.windows(seq.len()).any(|w| w == seq);

    // mysqldump noise: /*!40000 ALTER TABLE x DISABLE KEYS */
    if has(&["DISABLE", "KEYS"]) || has(&["ENABLE", "KEYS"]) {
        return Some(vec![Change::Session {
            sql: sql.to_string(),
        }]);
    }

    match words.first().copied()? {
        "CREATE" => create_object(schema, &toks, sql, dialect),
        "DROP" => drop_object(schema, &toks, dialect),
        "ALTER" if dialect == Dialect::MySql && words.get(1) == Some(&"TABLE") => {
            mysql_table_options(schema, &toks, dialect)
        }
        _ => None,
    }
}

const OBJECT_KINDS: [&str; 4] = ["PROCEDURE", "FUNCTION", "TRIGGER", "EVENT"];

fn create_object(
    schema: &Schema,
    toks: &[Tok],
    _sql: &str,
    dialect: Dialect,
) -> Option<Vec<Change>> {
    let mut i = 1;
    // Skip modifiers: OR REPLACE, TEMP/TEMPORARY, DEFINER = user, ALGORITHM = x, SQL SECURITY x
    loop {
        match toks.get(i)?.upper.as_str() {
            "OR" | "REPLACE" | "TEMP" | "TEMPORARY" | "CONSTRAINT" => i += 1,
            "DEFINER" | "ALGORITHM" => {
                i += 2; // keyword, `=`
                // user may be `x`@`y` or CURRENT_USER[()]
                while toks.get(i + 1).is_some_and(|t| {
                    t.upper == "@" || t.upper.starts_with('@') || t.upper == "(" || t.upper == ")"
                }) {
                    i += 2;
                }
                i += 1;
            }
            "SQL" => i += 3, // SQL SECURITY DEFINER
            _ => break,
        }
    }
    let kind_word = toks.get(i)?.upper.clone();
    let (kind, mut j) = if kind_word == "VIRTUAL" && toks.get(i + 1)?.upper == "TABLE" {
        ("virtual table".to_string(), i + 2)
    } else if OBJECT_KINDS.contains(&kind_word.as_str()) {
        (kind_word.to_ascii_lowercase(), i + 1)
    } else {
        return None;
    };
    let mut if_not_exists = false;
    if toks.get(j).is_some_and(|t| t.upper == "IF") {
        if_not_exists = true;
        j += 3;
    }
    let (name, after) = read_name(toks, j, dialect)?;
    let table = if kind == "trigger" {
        toks[after..]
            .iter()
            .position(|t| t.upper == "ON")
            .and_then(|p| read_name(toks, after + p + 1, dialect))
            .map(|(t, _)| t)
    } else {
        None
    };
    let exists = schema
        .objects
        .iter()
        .any(|o| o.kind == kind && o.name == name);
    if exists && if_not_exists {
        return Some(vec![]);
    }
    Some(vec![Change::CreateObject {
        object: OtherObject { kind, name, table },
        replaced: exists,
    }])
}

fn drop_object(schema: &Schema, toks: &[Tok], dialect: Dialect) -> Option<Vec<Change>> {
    let kind_word = toks.get(1)?.upper.clone();
    let (kind, mut j) = if OBJECT_KINDS.contains(&kind_word.as_str()) {
        (kind_word.to_ascii_lowercase(), 2)
    } else {
        return None;
    };
    let mut if_exists = false;
    if toks.get(j).is_some_and(|t| t.upper == "IF") {
        if_exists = true;
        j += 2;
    }
    let (name, _) = read_name(toks, j, dialect)?;
    match schema
        .objects
        .iter()
        .find(|o| o.kind == kind && o.name == name)
    {
        Some(object) => Some(vec![Change::DropObject {
            object: object.clone(),
        }]),
        None if if_exists => Some(vec![]),
        None => None,
    }
}

/// Normalise MySQL table-option names to the spelling `CREATE TABLE` options use.
#[must_use]
pub fn normalize_option_key(key: &str) -> String {
    let key = key
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_uppercase();
    match key.as_str() {
        "CHARSET" | "CHARACTER SET" | "DEFAULT CHARACTER SET" | "DEFAULT CHARSET" => {
            "DEFAULT CHARSET".into()
        }
        "DEFAULT COLLATE" | "COLLATE" => "COLLATE".into(),
        _ => key,
    }
}

/// MySQL `ALTER TABLE`s that `sqlparser` rejects because of table options, e.g.
/// `ALTER TABLE t ENGINE=InnoDB, ROW_FORMAT=DYNAMIC, COMMENT='…'`,
/// `ALTER TABLE t CONVERT TO CHARACTER SET x [COLLATE y]`, or a mix of those with ordinary
/// operations (`MODIFY COLUMN …, COMMENT=''`).
///
/// The statement is split into its comma-separated items: option items are handled here, and
/// every other item is re-parsed as its own `ALTER TABLE t <item>`.
fn mysql_table_options(schema: &Schema, toks: &[Tok], dialect: Dialect) -> Option<Vec<Change>> {
    let (table_name, start) = read_name(toks, 2, dialect)?;
    schema.table(&table_name)?;

    // Split the rest into items at top-level commas.
    let mut items: Vec<&[Tok]> = Vec::new();
    let (mut depth, mut item_start) = (0i32, start);
    for (i, t) in toks.iter().enumerate().skip(start) {
        match t.upper.as_str() {
            "(" => depth += 1,
            ")" => depth -= 1,
            "," | ";" if depth == 0 => {
                items.push(&toks[item_start..i]);
                item_start = i + 1;
            }
            _ => {}
        }
    }
    items.push(&toks[item_start..]);

    let mut scratch = schema.clone();
    let mut changes = Vec::new();
    for item in items.into_iter().filter(|i| !i.is_empty()) {
        let table = scratch.table(&table_name)?.clone();
        let item_changes = if let Some(c) = table_option_item(&table, item) {
            c
        } else {
            let text: Vec<&str> = item.iter().map(|t| t.text.as_str()).collect();
            let sql = format!(
                "ALTER TABLE {} {}",
                dialect.quote(&table_name),
                text.join(" ")
            );
            let mut parsed = Parser::parse_sql(dialect.parser_dialect(), &sql).ok()?;
            if parsed.len() != 1 {
                return None;
            }
            changes_for(&scratch, &parsed.remove(0), dialect, &sql)
        };
        for change in &item_changes {
            change.apply_to(&mut scratch);
        }
        changes.extend(item_changes);
    }
    Some(changes)
}

/// One table-option item (`ENGINE=InnoDB`, `COMMENT 'x'`, `CONVERT TO CHARACTER SET …`).
/// `None` if the item isn't a table option.
fn table_option_item(table: &Table, item: &[Tok]) -> Option<Vec<Change>> {
    let first = item.first()?.upper.as_str();
    let table_name = table.name.clone();

    if first == "CONVERT" {
        // CONVERT TO CHARACTER SET cs [COLLATE c]   |   CONVERT TO CHARSET cs …
        let mut i = 2;
        i += if item.get(i)?.upper == "CHARACTER" {
            2
        } else {
            1
        };
        let mut options = table.options.clone();
        options.insert("DEFAULT CHARSET".into(), item.get(i)?.original.clone());
        options.remove("COLLATE");
        if item.get(i + 1).is_some_and(|t| t.upper == "COLLATE") {
            options.insert("COLLATE".into(), item.get(i + 2)?.original.clone());
        }
        // CONVERT re-encodes every character column to the table default. Going from a
        // single-byte charset to a multi-byte one, MySQL also promotes TEXT types one size up so
        // they can still hold as many characters (TEXT → MEDIUMTEXT, …).
        let widening = is_single_byte(table.options.get("DEFAULT CHARSET").map(String::as_str))
            && !is_single_byte(options.get("DEFAULT CHARSET").map(String::as_str));
        let mut changes: Vec<Change> = table
            .columns
            .iter()
            .filter_map(|c| {
                let promoted = if widening {
                    promote_text(&c.data_type)
                } else {
                    None
                };
                if c.charset.is_none() && c.collation.is_none() && promoted.is_none() {
                    return None;
                }
                Some(Change::AlterColumn {
                    table: table_name.clone(),
                    before: c.clone(),
                    after: crate::schema::Column {
                        charset: None,
                        collation: None,
                        data_type: promoted.unwrap_or_else(|| c.data_type.clone()),
                        ..c.clone()
                    },
                })
            })
            .collect();
        changes.push(Change::SetTableOptions {
            table: table_name,
            before: table.options.clone(),
            after: options,
            convert: true,
        });
        return Some(changes);
    }

    // key [=] value, where the key may be several words (DEFAULT CHARSET, CHARACTER SET)
    let eq = item.iter().position(|t| t.upper == "=");
    let (key_toks, value_tok) = match eq {
        Some(p) => (&item[..p], item.get(p + 1)?),
        None if item.len() >= 2 => (&item[..item.len() - 1], item.last()?),
        None => return None,
    };
    if key_toks.is_empty() || !key_toks.iter().all(|t| t.is_word) {
        return None;
    }
    let key = normalize_option_key(
        &key_toks
            .iter()
            .map(|t| t.upper.as_str())
            .collect::<Vec<_>>()
            .join(" "),
    );
    let known = matches!(
        key.as_str(),
        "ENGINE"
            | "ROW_FORMAT"
            | "COMMENT"
            | "DEFAULT CHARSET"
            | "COLLATE"
            | "AUTO_INCREMENT"
            | "ALGORITHM"
            | "LOCK"
            | "KEY_BLOCK_SIZE"
            | "STATS_PERSISTENT"
            | "PACK_KEYS"
            | "CHECKSUM"
    );
    if !known {
        return None;
    }
    let value = value_tok.original.clone();
    Some(match key.as_str() {
        // How the ALTER runs / a counter: not part of the schema.
        "ALGORITHM" | "LOCK" | "AUTO_INCREMENT" => vec![],
        "COMMENT" => {
            let after = (!value.is_empty()).then_some(value);
            if after == table.comment {
                vec![]
            } else {
                vec![Change::SetComment {
                    table: table_name,
                    column: None,
                    before: table.comment.clone(),
                    after,
                }]
            }
        }
        _ => {
            let mut options = table.options.clone();
            if value.eq_ignore_ascii_case("DEFAULT") {
                options.remove(&key);
            } else {
                options.insert(key, value);
            }
            if options == table.options {
                vec![]
            } else {
                vec![Change::SetTableOptions {
                    table: table_name,
                    before: table.options.clone(),
                    after: options,
                    convert: false,
                }]
            }
        }
    })
}

fn is_single_byte(charset: Option<&str>) -> bool {
    matches!(
        charset.map(str::to_ascii_lowercase).as_deref(),
        Some(
            "latin1"
                | "ascii"
                | "latin2"
                | "latin5"
                | "latin7"
                | "cp1250"
                | "cp1251"
                | "cp1256"
                | "cp1257"
                | "binary"
        )
    )
}

/// The next-larger TEXT type, for MySQL's charset-conversion promotion.
fn promote_text(data_type: &str) -> Option<String> {
    Some(
        match data_type.to_ascii_uppercase().as_str() {
            "TINYTEXT" => "TEXT",
            "TEXT" => "MEDIUMTEXT",
            "MEDIUMTEXT" => "LONGTEXT",
            _ => return None,
        }
        .to_string(),
    )
}

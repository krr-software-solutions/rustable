//! Split a migration file into statements, and parse each one on its own.
//!
//! `sqlparser`'s `Parser::parse_sql` gives up on a whole file at the first statement it can't
//! parse, so instead we tokenize the file, cut it into statements at top-level `;`s, and parse
//! each piece separately. A statement that fails to parse is kept (with its original text), so
//! it can still be executed and reported on.
//!
//! Two things make "split on `;`" harder than it sounds:
//! - trigger bodies (`CREATE TRIGGER … BEGIN …; …; END;`) contain `;`s, so we track
//!   `BEGIN`/`CASE` … `END` depth inside a `CREATE … TRIGGER` statement;
//! - MySQL scripts use the client-side `DELIMITER $$` directive, which isn't SQL at all; we
//!   pre-split those regions on the custom delimiter before tokenizing anything.
//!
//! Strings, quoted identifiers, comments and Postgres `$$…$$` bodies are single tokens, so a `;`
//! inside them never splits a statement.

use sqlparser::ast::Statement;
use sqlparser::keywords::Keyword;
use sqlparser::parser::Parser;
use sqlparser::tokenizer::{Token, Tokenizer, Whitespace};

use crate::dialect::Dialect;

/// One statement from a migration file.
#[derive(Clone, Debug)]
pub struct SqlStatement {
    /// 1-based line where the statement starts in its file.
    pub line: u64,
    /// The statement's original text, without the trailing delimiter.
    pub sql: String,
    /// The parsed statement, or the parser's error message.
    pub parsed: Result<Statement, String>,
}

impl SqlStatement {
    /// The first keyword of the statement, uppercased (`CREATE`, `INSERT`, `PRAGMA`, …).
    #[must_use]
    pub fn leading_keyword(&self) -> String {
        first_word(&self.sql).to_ascii_uppercase()
    }
}

/// Split `text` into statements and parse each one.
#[must_use]
pub fn split_and_parse(text: &str, dialect: Dialect) -> Vec<SqlStatement> {
    let mut out = Vec::new();
    for segment in delimiter_segments(text, dialect) {
        match segment.delimiter {
            None => split_tokens(segment.text, segment.start_line, dialect, &mut out),
            Some(delimiter) => {
                let mut line = segment.start_line;
                for piece in segment.text.split(delimiter.as_str()) {
                    push_statement(piece, line + leading_blank_lines(piece), dialect, &mut out);
                    line += count_newlines(piece);
                }
            }
        }
    }
    out
}

struct Segment<'a> {
    text: &'a str,
    start_line: u64,
    /// `None` = the normal `;` delimiter, handled by the tokenizer.
    delimiter: Option<String>,
}

/// Cut the text into regions by MySQL `DELIMITER x` lines. Other dialects get one segment.
fn delimiter_segments(text: &str, dialect: Dialect) -> Vec<Segment<'_>> {
    if dialect != Dialect::MySql {
        return vec![Segment {
            text,
            start_line: 1,
            delimiter: None,
        }];
    }

    let mut segments = Vec::new();
    let mut delimiter: Option<String> = None;
    let mut seg_start_byte = 0;
    let mut seg_start_line = 1;
    let mut byte = 0;
    let mut line_no: u64 = 1;

    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        let directive = trimmed
            .get(..9)
            .filter(|w| w.eq_ignore_ascii_case("DELIMITER"))
            .and_then(|_| trimmed[9..].split_whitespace().next());
        if let Some(new_delimiter) = directive {
            segments.push(Segment {
                text: &text[seg_start_byte..byte],
                start_line: seg_start_line,
                delimiter: delimiter.clone(),
            });
            delimiter = (new_delimiter != ";").then(|| new_delimiter.to_string());
            seg_start_byte = byte + line.len();
            seg_start_line = line_no + 1;
        }
        byte += line.len();
        line_no += 1;
    }
    segments.push(Segment {
        text: &text[seg_start_byte..],
        start_line: seg_start_line,
        delimiter,
    });
    segments
}

fn split_tokens(text: &str, start_line: u64, dialect: Dialect, out: &mut Vec<SqlStatement>) {
    let tokens = match Tokenizer::new(dialect.parser_dialect(), text).tokenize_with_location() {
        Ok(tokens) => tokens,
        Err(e) => {
            // Can't even tokenize (e.g. an unterminated string): keep the whole segment as one
            // unparsed statement rather than guessing at boundaries.
            if !is_blank(text) {
                out.push(SqlStatement {
                    line: start_line + leading_blank_lines(text),
                    sql: text.trim().to_string(),
                    parsed: Err(format!("tokenizer error: {e}")),
                });
            }
            return;
        }
    };

    let mut current = String::new();
    let mut first_line: Option<u64> = None;
    let mut words: Vec<Keyword> = Vec::new(); // significant keywords seen so far in this statement
    let mut depth: u32 = 0;

    for t in tokens {
        let is_ws = matches!(t.token, Token::Whitespace(_));
        if t.token == Token::SemiColon && depth == 0 {
            push_statement_at(&current, first_line, start_line, dialect, out);
            current.clear();
            first_line = None;
            words.clear();
            continue;
        }

        if !is_ws && first_line.is_none() {
            first_line = Some(t.span.start.line);
        }
        if let Token::Word(w) = &t.token {
            let in_trigger =
                words.first() == Some(&Keyword::CREATE) && words.contains(&Keyword::TRIGGER);
            match w.keyword {
                Keyword::BEGIN | Keyword::CASE if in_trigger => depth += 1,
                Keyword::END if depth > 0 => depth -= 1,
                _ => {}
            }
            if w.keyword != Keyword::NoKeyword {
                words.push(w.keyword);
            }
        }
        if first_line.is_some() {
            current.push_str(&t.token.to_string());
        }
    }
    push_statement_at(&current, first_line, start_line, dialect, out);
}

fn push_statement_at(
    text: &str,
    first_line: Option<u64>,
    start_line: u64,
    dialect: Dialect,
    out: &mut Vec<SqlStatement>,
) {
    if let Some(line) = first_line {
        push_statement(text, start_line + line - 1, dialect, out);
    }
}

fn push_statement(text: &str, line: u64, dialect: Dialect, out: &mut Vec<SqlStatement>) {
    if is_blank(text) {
        return;
    }
    let sql = text.trim().to_string();
    let parsed = match Parser::parse_sql(dialect.parser_dialect(), &sql) {
        Ok(mut statements) if statements.len() == 1 => Ok(statements.remove(0)),
        Ok(statements) if statements.is_empty() => return, // only comments
        Ok(statements) => Err(format!(
            "expected one statement, parsed {}",
            statements.len()
        )),
        Err(e) => Err(e.to_string()),
    };
    out.push(SqlStatement { line, sql, parsed });
}

/// True if `text` is only whitespace and comments.
fn is_blank(text: &str) -> bool {
    match Tokenizer::new(&sqlparser::dialect::GenericDialect {}, text).tokenize() {
        Ok(tokens) => tokens.iter().all(|t| matches!(t, Token::Whitespace(_))),
        Err(_) => text.trim().is_empty(),
    }
}

/// First word of the statement, skipping leading whitespace and comments.
fn first_word(sql: &str) -> String {
    if let Ok(tokens) = Tokenizer::new(&sqlparser::dialect::GenericDialect {}, sql).tokenize() {
        for t in tokens {
            match t {
                Token::Whitespace(
                    Whitespace::Space
                    | Whitespace::Newline
                    | Whitespace::Tab
                    | Whitespace::SingleLineComment { .. }
                    | Whitespace::MultiLineComment(_),
                ) => {}
                Token::Word(w) => return w.value,
                other => return other.to_string(),
            }
        }
    }
    sql.split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string()
}

/// The table a data statement writes to (`INSERT [IGNORE] INTO t`, `UPDATE t`, `DELETE FROM t`),
/// normalised like table names elsewhere.
#[must_use]
pub fn data_target(sql: &str, dialect: Dialect) -> Option<String> {
    let tokens = Tokenizer::new(dialect.parser_dialect(), sql)
        .tokenize()
        .ok()?;
    let words: Vec<(String, String)> = tokens
        .iter()
        .filter(|t| !matches!(t, Token::Whitespace(_)))
        .map(|t| match t {
            Token::Word(w) => (
                w.value.to_ascii_uppercase(),
                if w.quote_style.is_some() {
                    w.value.clone()
                } else {
                    w.value.to_lowercase()
                },
            ),
            other => (other.to_string(), other.to_string()),
        })
        .collect();
    let start = match words.first()?.0.as_str() {
        "INSERT" | "REPLACE" => words.iter().position(|(u, _)| u == "INTO")? + 1,
        "UPDATE" => {
            1 + usize::from(matches!(
                words.get(1)?.0.as_str(),
                "LOW_PRIORITY" | "IGNORE"
            ))
        }
        "DELETE" => words.iter().position(|(u, _)| u == "FROM")? + 1,
        _ => return None,
    };
    let mut name = words.get(start)?.1.clone();
    let mut i = start + 1;
    while words.get(i).is_some_and(|(u, _)| u == ".") {
        name.push('.');
        name.push_str(&words.get(i + 1)?.1);
        i += 2;
    }
    Some(match dialect {
        Dialect::Postgres => name
            .strip_prefix("public.")
            .map_or(name.clone(), str::to_string),
        _ => name,
    })
}

/// The columns an `UPDATE … SET a = …, b = …` assigns (unqualified, normalised), or `None`
/// if the statement isn't an UPDATE.
#[must_use]
pub fn update_assigned_columns(sql: &str, dialect: Dialect) -> Option<Vec<String>> {
    let tokens = Tokenizer::new(dialect.parser_dialect(), sql)
        .tokenize()
        .ok()?;
    let toks: Vec<&Token> = tokens
        .iter()
        .filter(|t| !matches!(t, Token::Whitespace(_)))
        .collect();
    let is_kw = |t: &Token, kw: &str| matches!(t, Token::Word(w) if w.quote_style.is_none() && w.value.eq_ignore_ascii_case(kw));
    if !toks.first().is_some_and(|t| is_kw(t, "UPDATE")) {
        return None;
    }
    let mut depth = 0i32;
    let mut i = toks.iter().position(|t| is_kw(t, "SET"))? + 1;
    let mut columns = Vec::new();
    let mut expect_column = true;
    while i < toks.len() {
        let t = toks[i];
        match t {
            Token::LParen => depth += 1,
            Token::RParen => depth -= 1,
            Token::Comma if depth == 0 => expect_column = true,
            Token::Word(w)
                if depth == 0
                    && w.quote_style.is_none()
                    && ["WHERE", "FROM", "RETURNING", "ORDER", "LIMIT"]
                        .iter()
                        .any(|k| w.value.eq_ignore_ascii_case(k)) =>
            {
                break;
            }
            Token::Word(w) if depth == 0 && expect_column => {
                // `t.col` → take the part right before `=`
                let mut name = w.clone();
                while matches!(toks.get(i + 1), Some(Token::Period)) {
                    if let Some(Token::Word(next)) = toks.get(i + 2) {
                        name = next.clone();
                        i += 2;
                    } else {
                        break;
                    }
                }
                columns.push(if name.quote_style.is_some() {
                    name.value
                } else {
                    name.value.to_lowercase()
                });
                expect_column = false;
            }
            _ => {}
        }
        i += 1;
    }
    Some(columns)
}

fn count_newlines(s: &str) -> u64 {
    s.bytes().filter(|&b| b == b'\n').count() as u64
}

fn leading_blank_lines(s: &str) -> u64 {
    let trimmed = s.trim_start();
    count_newlines(&s[..s.len() - trimmed.len()])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sqls(text: &str, dialect: Dialect) -> Vec<(u64, String, bool)> {
        split_and_parse(text, dialect)
            .into_iter()
            .map(|s| (s.line, s.sql, s.parsed.is_ok()))
            .collect()
    }

    #[test]
    fn splits_and_recovers_from_bad_statements() {
        let text = "CREATE TABLE a (id INT);\n-- comment\nDO $$ BEGIN RAISE NOTICE 'x;y'; END $$;\nCREATE TABLE b (id INT);\n";
        let out = sqls(text, Dialect::Postgres);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0], (1, "CREATE TABLE a (id INT)".to_string(), true));
        assert_eq!(out[1].0, 3);
        assert!(out[1].1.starts_with("DO $$"));
        assert!(!out[1].2, "DO blocks don't parse in sqlparser 0.63");
        assert_eq!(out[2], (4, "CREATE TABLE b (id INT)".to_string(), true));
    }

    #[test]
    fn keeps_trigger_bodies_together() {
        let text = "CREATE TRIGGER t AFTER UPDATE ON notes BEGIN\n  UPDATE notes SET n = 1;\n  INSERT INTO log VALUES (1);\nEND;\nCREATE INDEX i ON notes (n);";
        let out = sqls(text, Dialect::Sqlite);
        assert_eq!(out.len(), 2, "{out:#?}");
        assert!(out[0].1.ends_with("END"));
        assert_eq!(out[1].0, 5);
    }

    #[test]
    fn handles_mysql_delimiter_blocks() {
        let text = "CREATE TABLE a (id INT);\nDELIMITER $$\nCREATE PROCEDURE p() BEGIN SELECT 1; SELECT 2; END$$\nDELIMITER ;\nCREATE TABLE b (id INT);\n";
        let out = sqls(text, Dialect::MySql);
        assert_eq!(out.len(), 3, "{out:#?}");
        assert!(out[1].1.starts_with("CREATE PROCEDURE"));
        assert!(out[1].1.ends_with("END"));
        assert_eq!(out[1].0, 3);
        assert_eq!(out[2].1, "CREATE TABLE b (id INT)");
        assert_eq!(out[2].0, 5);
    }

    #[test]
    fn finds_update_assignments() {
        let cols = update_assigned_columns(
            "UPDATE `users` u SET u.display_name = SUBSTRING_INDEX(email, '@', 1), `nick` = lower(x, y) WHERE id > 0",
            Dialect::MySql,
        );
        assert_eq!(
            cols,
            Some(vec!["display_name".to_string(), "nick".to_string()])
        );
        assert_eq!(
            update_assigned_columns("INSERT INTO t VALUES (1)", Dialect::MySql),
            None
        );
    }

    #[test]
    fn leading_keyword_skips_comments() {
        let out = split_and_parse(
            "/* hi */ -- there\n  pragma foreign_keys = ON;",
            Dialect::Sqlite,
        );
        assert_eq!(out[0].leading_keyword(), "PRAGMA");
    }
}

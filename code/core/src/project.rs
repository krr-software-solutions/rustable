//! Analyze a whole migrations directory: discover, parse, fold, classify, and check undos.

use std::path::{Path, PathBuf};

use crate::analysis::{
    Compat, DeployPhase, Reversibility, backward_compat, forward_compat, reversibility,
};
use crate::apply::apply_statement;
use crate::change::Change;
use crate::dialect::Dialect;
use crate::diff::diff;
use crate::discover::discover;
use crate::error::Error;
use crate::schema::Schema;
use crate::statements::split_and_parse;
use crate::undo::{GeneratedUndo, generate_undo};
use crate::version::MigrationId;

/// One change made by one statement, with its classification.
#[derive(Clone, Debug)]
pub struct Step {
    pub line: u64,
    pub sql: String,
    pub parse_error: Option<String>,
    pub change: Change,
    pub reversibility: Reversibility,
    pub backward: Compat,
    pub forward: Compat,
}

/// The result of checking an undo script against its migration.
#[derive(Clone, Debug)]
pub struct UndoCheck {
    /// Differences between the schema before the migration and after migration + undo.
    /// Empty means the undo restores the schema exactly.
    pub differences: Vec<String>,
    /// How reversible the undo itself is (can it be re-done without loss?).
    pub reversibility: Reversibility,
    pub steps: Vec<Step>,
}

impl UndoCheck {
    #[must_use]
    pub fn restores_schema(&self) -> bool {
        self.differences.is_empty()
    }
}

#[derive(Clone, Debug)]
pub struct MigrationReport {
    pub id: MigrationId,
    pub up_path: Option<PathBuf>,
    pub undo_path: Option<PathBuf>,
    pub steps: Vec<Step>,
    pub reversibility: Reversibility,
    pub backward: Compat,
    pub forward: Compat,
    pub phase: DeployPhase,
    /// Checking the provided `.undo.sql`, if there is one.
    pub provided_undo: Option<UndoCheck>,
    /// What rustable would generate as an undo (verified against the schema).
    pub generated_undo: GeneratedUndo,
}

impl MigrationReport {
    /// Statements that failed to parse.
    pub fn parse_errors(&self) -> impl Iterator<Item = &Step> {
        self.steps.iter().filter(|s| s.parse_error.is_some())
    }
}

#[derive(Clone, Debug)]
pub struct Project {
    pub dialect: Dialect,
    pub migrations: Vec<MigrationReport>,
    /// The schema after every migration.
    pub schema: Schema,
}

/// The text of one migration (for analysis without touching the filesystem).
#[derive(Clone, Debug)]
pub struct MigrationSource {
    pub id: MigrationId,
    pub up: String,
    pub undo: Option<String>,
    pub up_path: Option<PathBuf>,
    pub undo_path: Option<PathBuf>,
}

/// Discover and analyze every migration in `dir`.
///
/// # Errors
/// I/O errors and malformed migration file names. SQL problems are reported in the result, not
/// as errors.
pub fn analyze_dir(dir: &Path, dialect: Dialect) -> Result<Project, Error> {
    let read = |path: &Path| {
        std::fs::read_to_string(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })
    };
    let mut sources = Vec::new();
    for files in discover(dir)? {
        sources.push(MigrationSource {
            up: read(&files.up)?,
            undo: files.undo.as_deref().map(read).transpose()?,
            id: files.id,
            up_path: Some(files.up),
            undo_path: files.undo,
        });
    }
    Ok(analyze(sources, dialect))
}

/// Fold over migrations in order, analyzing each.
#[must_use]
pub fn analyze(sources: Vec<MigrationSource>, dialect: Dialect) -> Project {
    let mut schema = Schema::default();
    let mut migrations = Vec::new();

    for source in sources {
        let before = schema.clone();
        let steps = run(&mut schema, &source.up, dialect);
        let changes: Vec<Change> = steps.iter().map(|s| s.change.clone()).collect();

        let provided_undo = source
            .undo
            .as_deref()
            .map(|text| check_undo(&before, &schema, text, dialect));
        let generated_undo = match generate_undo(&changes, dialect) {
            GeneratedUndo::Generated(script) => {
                let check = check_undo(&before, &schema, &script, dialect);
                if check.restores_schema() {
                    GeneratedUndo::Generated(script)
                } else {
                    GeneratedUndo::NotTrivial(
                        check
                            .differences
                            .into_iter()
                            .map(|d| format!("generated undo would leave a difference: {d}"))
                            .collect(),
                    )
                }
            }
            not => not,
        };

        let reversibility = Reversibility::worst(steps.iter().map(|s| &s.reversibility));
        let backward = Compat::worst(steps.iter().map(|s| &s.backward));
        let forward = Compat::worst(steps.iter().map(|s| &s.forward));
        let phase = DeployPhase::from_compat(&backward, &forward);

        migrations.push(MigrationReport {
            id: source.id,
            up_path: source.up_path,
            undo_path: source.undo_path,
            steps,
            reversibility,
            backward,
            forward,
            phase,
            provided_undo,
            generated_undo,
        });
    }

    schema.normalize();
    Project {
        dialect,
        migrations,
        schema,
    }
}

/// Apply a script to `schema`, returning one classified step per change.
fn run(schema: &mut Schema, text: &str, dialect: Dialect) -> Vec<Step> {
    let mut steps = Vec::new();
    // Tables created by this script: writing data into them (seed rows) is harmless, since
    // undoing the script drops the table and its rows together.
    let mut created: Vec<String> = Vec::new();
    // Columns added by this script: an UPDATE that only fills them (a backfill) goes away with them.
    let mut added: Vec<(String, String)> = Vec::new();
    for stmt in split_and_parse(text, dialect) {
        for change in apply_statement(schema, &stmt, dialect) {
            match &change {
                Change::CreateTable { table } => created.push(table.name.clone()),
                Change::RenameTable { from, to } if created.contains(from) => {
                    created.push(to.clone());
                }
                _ => {}
            }
            if let Change::AddColumn { table, column, .. } = &change {
                added.push((table.clone(), column.name.clone()));
            }
            let seeds_new_table = matches!(change, Change::Data { .. })
                && (crate::statements::data_target(&stmt.sql, dialect)
                    .is_some_and(|t| created.contains(&t))
                    || crate::undo::backfills_new_columns(&stmt.sql, dialect, &added));
            let (rev, backward, forward) = if seeds_new_table {
                (Reversibility::Lossless, Compat::Yes, Compat::Yes)
            } else {
                (
                    reversibility(&change),
                    backward_compat(&change),
                    forward_compat(&change),
                )
            };
            steps.push(Step {
                line: stmt.line,
                sql: stmt.sql.clone(),
                parse_error: stmt.parsed.as_ref().err().cloned(),
                reversibility: rev,
                backward,
                forward,
                change,
            });
        }
    }
    steps
}

fn check_undo(before: &Schema, after: &Schema, undo_text: &str, dialect: Dialect) -> UndoCheck {
    let mut undone = after.clone();
    let steps = run(&mut undone, undo_text, dialect);
    let mut expected = before.clone();
    expected.normalize();
    undone.normalize();
    UndoCheck {
        differences: diff(&expected, &undone),
        reversibility: Reversibility::worst(steps.iter().map(|s| &s.reversibility)),
        steps,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::version::parse_file_name;

    fn source(file: &str, up: &str, undo: Option<&str>) -> MigrationSource {
        MigrationSource {
            id: parse_file_name(file).unwrap().unwrap().0,
            up: up.to_string(),
            undo: undo.map(str::to_string),
            up_path: None,
            undo_path: None,
        }
    }

    #[test]
    fn folds_and_checks_undos() {
        let project = analyze(
            vec![
                source(
                    "V2025_01_01__00_00__create-users.sql",
                    "CREATE TABLE users (id bigserial PRIMARY KEY, email text NOT NULL);
                     CREATE UNIQUE INDEX users_email_idx ON users (lower(email));",
                    Some("DROP TABLE users;"),
                ),
                source(
                    "V2025_01_02__00_00__add-name.sql",
                    "ALTER TABLE users ADD COLUMN name text;",
                    None,
                ),
                source(
                    "V2025_01_03__00_00__drop-email.sql",
                    "ALTER TABLE users DROP COLUMN email;",
                    Some("ALTER TABLE users ADD COLUMN email text;"),
                ),
            ],
            Dialect::Postgres,
        );

        let [create, add, drop] = &project.migrations[..] else {
            panic!()
        };

        assert!(create.provided_undo.as_ref().unwrap().restores_schema());
        assert_eq!(create.phase, DeployPhase::Expand);
        assert!(create.reversibility.is_lossless());

        assert_eq!(add.phase, DeployPhase::Expand);
        let GeneratedUndo::Generated(script) = &add.generated_undo else {
            panic!("{:?}", add.generated_undo)
        };
        assert!(
            script.contains("ALTER TABLE users DROP COLUMN name;"),
            "{script}"
        );

        assert_eq!(drop.phase, DeployPhase::Contract);
        assert!(matches!(drop.reversibility, Reversibility::Lossy(_)));
        assert!(matches!(drop.generated_undo, GeneratedUndo::NotTrivial(_)));
        // The provided undo forgets NOT NULL and the unique index on email.
        let check = drop.provided_undo.as_ref().unwrap();
        assert!(!check.restores_schema());
        assert_eq!(check.differences.len(), 2, "{:#?}", check.differences);

        assert_eq!(
            project
                .schema
                .table("users")
                .unwrap()
                .columns
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            vec!["id", "name"]
        );
    }
}

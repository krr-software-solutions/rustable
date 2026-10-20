//! Run the analyzer over every example in `examples/` and check it holds up.

use std::path::{Path, PathBuf};

use rustable_core::Dialect;
use rustable_core::project::analyze_dir;

fn examples() -> Vec<(PathBuf, Dialect)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut out = Vec::new();
    for (dir, dialect) in [
        ("mysql", Dialect::MySql),
        ("postgres", Dialect::Postgres),
        ("sqlite3", Dialect::Sqlite),
    ] {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(root.join(dir))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.is_dir())
            .collect();
        entries.sort();
        out.extend(entries.into_iter().map(|p| (p, dialect)));
    }
    out
}

/// The examples' READMEs each document one deliberately incomplete undo per dialect (two for
/// Postgres). Those, and only those, must be flagged.
const KNOWN_BAD_UNDOS: [&str; 4] = [
    "V2024_10_01__11_45__orders-customer-created-index",
    "V2025_01_04__11_25__narrow-recipe-columns",
    "V2025_03_17__10_40__add-products-sku",
    "V2025_01_18__13_40__rebuild-bookmarks",
];

#[test]
fn flags_exactly_the_known_bad_undos() {
    let mut flagged = Vec::new();
    for (dir, dialect) in examples() {
        let project = analyze_dir(&dir, dialect).unwrap();
        for m in &project.migrations {
            if m.provided_undo
                .as_ref()
                .is_some_and(|c| !c.restores_schema())
            {
                flagged.push(m.id.to_string());
            }
        }
    }
    flagged.sort();
    let mut expected: Vec<String> = KNOWN_BAD_UNDOS.iter().map(ToString::to_string).collect();
    expected.sort();
    assert_eq!(flagged, expected);
}

#[test]
fn only_do_blocks_are_unknown() {
    for (dir, dialect) in examples() {
        let project = analyze_dir(&dir, dialect).unwrap();
        for m in &project.migrations {
            for s in &m.steps {
                if let rustable_core::change::Change::Unknown { reason, sql } = &s.change {
                    assert!(
                        sql.starts_with("DO "),
                        "{} {}: {reason}",
                        dir.display(),
                        m.id
                    );
                }
            }
        }
    }
}

#[test]
fn every_example_analyzes() {
    for (dir, dialect) in examples() {
        let project =
            analyze_dir(&dir, dialect).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
        assert!(!project.migrations.is_empty(), "{}", dir.display());
        println!("\n=== {} ({dialect})", dir.display());
        for m in &project.migrations {
            println!(
                "  {}  rev={} back={} fwd={} phase={:?} undo={}",
                m.id,
                m.reversibility.label(),
                m.backward.label(),
                m.forward.label(),
                m.phase,
                match (&m.provided_undo, &m.generated_undo) {
                    (Some(c), _) if c.restores_schema() => "provided ✓".to_string(),
                    (Some(c), _) => format!("provided ✗ {:?}", c.differences),
                    (None, rustable_core::undo::GeneratedUndo::Generated(_)) =>
                        "generated".to_string(),
                    (None, rustable_core::undo::GeneratedUndo::NotTrivial(r)) =>
                        format!("none ({} reasons)", r.len()),
                }
            );
            for s in &m.steps {
                if let rustable_core::change::Change::Unknown { reason, .. } = &s.change {
                    println!("      UNKNOWN line {}: {reason}", s.line);
                }
            }
        }
    }
}

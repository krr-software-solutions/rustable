//! Find migration files in a directory and pair each up file with its undo file.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::version::{FileKind, MigrationId, Version, parse_file_name};

/// One migration on disk: the up file, and the undo file if there is one.
#[derive(Clone, Debug)]
pub struct MigrationFiles {
    pub id: MigrationId,
    pub up: PathBuf,
    pub undo: Option<PathBuf>,
}

/// List the migrations in `dir`, in version order.
///
/// # Errors
/// I/O errors, malformed `V….sql` names, two migrations sharing a version, or an undo file
/// without a matching up file.
pub fn discover(dir: &Path) -> Result<Vec<MigrationFiles>, Error> {
    let io = |source| Error::Io {
        path: dir.to_path_buf(),
        source,
    };

    let mut ups: BTreeMap<Version, (MigrationId, PathBuf)> = BTreeMap::new();
    let mut undos: Vec<(MigrationId, PathBuf)> = Vec::new();

    for entry in std::fs::read_dir(dir).map_err(io)? {
        let entry = entry.map_err(io)?;
        if !entry.file_type().map_err(io)?.is_file() {
            continue;
        }
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        let Some((id, kind)) = parse_file_name(file_name)? else {
            continue;
        };
        match kind {
            FileKind::Up => {
                if let Some((existing, _)) = ups.get(&id.version) {
                    return Err(Error::DuplicateVersion {
                        version: id.version.to_string(),
                        first: existing.up_file_name(),
                        second: id.up_file_name(),
                    });
                }
                ups.insert(id.version, (id, entry.path()));
            }
            FileKind::Undo => undos.push((id, entry.path())),
        }
    }

    let mut migrations: BTreeMap<Version, MigrationFiles> = ups
        .into_iter()
        .map(|(version, (id, up))| (version, MigrationFiles { id, up, undo: None }))
        .collect();

    for (id, path) in undos {
        match migrations.get_mut(&id.version) {
            Some(m) if m.id == id => m.undo = Some(path),
            _ => {
                return Err(Error::OrphanUndo {
                    name: id.undo_file_name(),
                });
            }
        }
    }

    Ok(migrations.into_values().collect())
}

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to read {}: {source}", path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("invalid migration file name {name:?}: {reason}")]
    InvalidFileName { name: String, reason: String },

    #[error("duplicate migration version {version}: {first:?} and {second:?}")]
    DuplicateVersion {
        version: String,
        first: String,
        second: String,
    },

    #[error("undo file {name:?} has no matching migration")]
    OrphanUndo { name: String },

    #[error("failed to render schema as TOML: {0}")]
    SchemaToml(#[from] toml::ser::Error),
}

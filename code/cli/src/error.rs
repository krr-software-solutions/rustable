use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
pub enum RustableError {
    Generic {
        message: String,
    },
    ConfigRead {
        path: PathBuf,
        source: std::io::Error,
    },
    ConfigParse {
        path: PathBuf,
        source: toml::de::Error,
    },
    Core(rustable_sql_core::Error),
    Db(rustable_sql_db::DbError),
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
    /// `rustable check` found problems (already printed).
    CheckFailed {
        problems: usize,
    },
}

impl fmt::Display for RustableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RustableError::Generic { message } => f.write_str(message),
            RustableError::ConfigRead { path, source } => {
                write!(f, "failed to read config {}: {source}", path.display())
            }
            RustableError::ConfigParse { path, source } => {
                write!(f, "failed to parse config {}: {source}", path.display())
            }
            RustableError::Core(e) => write!(f, "{e}"),
            RustableError::Db(e) => write!(f, "{e}"),
            RustableError::Write { path, source } => {
                write!(f, "failed to write {}: {source}", path.display())
            }
            RustableError::CheckFailed { problems } => {
                write!(f, "check failed with {problems} problem(s)")
            }
        }
    }
}

impl From<rustable_sql_core::Error> for RustableError {
    fn from(e: rustable_sql_core::Error) -> Self {
        RustableError::Core(e)
    }
}

impl From<rustable_sql_db::DbError> for RustableError {
    fn from(e: rustable_sql_db::DbError) -> Self {
        RustableError::Db(e)
    }
}

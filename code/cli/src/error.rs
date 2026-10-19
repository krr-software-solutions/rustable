use std::path::PathBuf;

#[derive(Debug)]
pub enum RustableError {
    // TODO (KR) : actual errors...
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
}

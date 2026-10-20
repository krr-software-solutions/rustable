use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use crate::error::RustableError;

fn abs_canonical_path<'a>(base: &Path) -> Result<PathBuf, RustableError> {
    let absolute_buf: PathBuf =
        std::path::absolute(base).map_err(|source| RustableError::ConfigRead {
            path: base.to_path_buf(),
            source,
        })?;
    absolute_buf
        .canonicalize()
        .map_err(|source| RustableError::ConfigRead {
            path: base.to_path_buf(),
            source,
        })
}

/// Lexically removes `.` and `..` components, without touching the filesystem.
/// Intended for absolute paths: a leading `..` has nothing to pop and is dropped.
/// Note: unlike `canonicalize`, this does not resolve symlinks, so `link/..` becomes
/// the directory containing `link`, not the parent of the link's target.
fn normalize(path: &Path) -> PathBuf {
    let mut out: PathBuf = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// Like `abs_canonical_path`, but works for paths that don't exist yet.
fn abs_normalized_path(base: &Path) -> Result<PathBuf, RustableError> {
    let absolute_buf: PathBuf =
        std::path::absolute(base).map_err(|source| RustableError::ConfigRead {
            path: base.to_path_buf(),
            source,
        })?;
    Ok(normalize(absolute_buf.as_path()))
}

fn calculate_opt_relative_path(
    config_dir: &Path,
    opt_relative: Option<PathBuf>,
    default_relative: &str,
) -> Result<PathBuf, RustableError> {
    let base_path: PathBuf = match opt_relative {
        Some(relative) => config_dir.join(relative),
        None => config_dir.join(default_relative),
    };
    abs_normalized_path(base_path.as_path())
}

/// The dialect enum lives in `rustable-core`, so the CLI never has to touch `sqlparser`.
pub use rustable_core::Dialect;

// TODO (KR) : figure out what configs need to live where
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawInitialConfig {
    pub dialect: Dialect,
    pub migrations_dir_path: Option<PathBuf>,
    pub migrations_md_path: Option<PathBuf>,
    pub current_schema_path: Option<PathBuf>,
    /// Schema written by the app's `#[derive(Table)]` tables (`rustable_core::code`).
    pub code_schema_path: Option<PathBuf>,
    /// Where `rustable plan --write` reports differences it left alone.
    pub outstanding_path: Option<PathBuf>,
}
impl RawInitialConfig {
    pub async fn load(absolute_config_path: &Path) -> Result<RawInitialConfig, RustableError> {
        let text: String = tokio::fs::read_to_string(absolute_config_path)
            .await
            .map_err(|source| RustableError::ConfigRead {
                path: absolute_config_path.to_path_buf(),
                source,
            })?;

        toml::from_str(&text).map_err(|source| RustableError::ConfigParse {
            path: absolute_config_path.to_path_buf(),
            source,
        })
    }
}

#[derive(Clone, Debug)]
pub struct InitialConfig {
    pub config_path: PathBuf,
    pub dialect: Dialect,
    pub migrations_dir_path: PathBuf,
    pub migrations_md_path: PathBuf,
    pub current_schema_path: PathBuf,
    pub code_schema_path: PathBuf,
    pub outstanding_path: PathBuf,
}
impl InitialConfig {
    pub fn from_raw(
        absolute_config_path: &Path,
        raw: RawInitialConfig,
    ) -> Result<InitialConfig, RustableError> {
        let config_dir: &Path = absolute_config_path
            .parent()
            .ok_or(RustableError::Generic {
                message: "No parent???".to_string(),
            })?;

        let migrations_dir_path: PathBuf =
            calculate_opt_relative_path(config_dir, raw.migrations_dir_path, ".")?;
        let migrations_md_path: PathBuf =
            calculate_opt_relative_path(config_dir, raw.migrations_md_path, "./MIGRATIONS.md")?;
        let current_schema_path: PathBuf =
            calculate_opt_relative_path(config_dir, raw.current_schema_path, "./schema.toml")?;
        let code_schema_path: PathBuf = calculate_opt_relative_path(
            config_dir,
            raw.code_schema_path,
            "./code.current_schema.toml",
        )?;
        let outstanding_path: PathBuf =
            calculate_opt_relative_path(config_dir, raw.outstanding_path, "./OUTSTANDING.md")?;

        let cfg: InitialConfig = InitialConfig {
            config_path: absolute_config_path.to_path_buf(),
            dialect: raw.dialect,
            migrations_dir_path,
            migrations_md_path,
            current_schema_path,
            code_schema_path,
            outstanding_path,
        };
        Ok(cfg)
    }

    pub async fn load(any_config_path: &Path) -> Result<InitialConfig, RustableError> {
        let absolute_buf: PathBuf = abs_canonical_path(any_config_path)?;
        let absolute_config_path: &Path = absolute_buf.as_path();
        let raw: RawInitialConfig = RawInitialConfig::load(absolute_config_path).await?;
        InitialConfig::from_raw(absolute_config_path, raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(s: &str) -> PathBuf {
        normalize(Path::new(s))
    }

    #[test]
    fn normalize_removes_cur_and_parent_dirs() {
        assert_eq!(n("/a/b/../c/./d"), PathBuf::from("/a/c/d"));
        assert_eq!(
            n("/a/ex-1/../wowza/migrations/"),
            PathBuf::from("/a/wowza/migrations")
        );
        assert_eq!(n("/a/b/c/../../d"), PathBuf::from("/a/d"));
    }

    #[test]
    fn normalize_stops_at_root() {
        assert_eq!(n("/a/../../.."), PathBuf::from("/"));
    }

    #[test]
    fn normalize_leaves_clean_paths_alone() {
        assert_eq!(n("/a/b/c.toml"), PathBuf::from("/a/b/c.toml"));
    }
}

use std::path::{Path, PathBuf};

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

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dialect {
    Postgres,
    MySql,
    Sqlite,
}
impl Dialect {
    pub fn parser_dialect(self) -> &'static dyn sqlparser::dialect::Dialect {
        match self {
            Dialect::Postgres => &sqlparser::dialect::PostgreSqlDialect {},
            Dialect::MySql => &sqlparser::dialect::MySqlDialect {},
            Dialect::Sqlite => &sqlparser::dialect::SQLiteDialect {},
        }
    }
}

// TODO (KR) : figure out what configs need to live where
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawInitialConfig {
    pub dialect: Dialect,
    pub migrations_dir: PathBuf,
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
    pub migrations_dir: PathBuf,
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
        let migrations_dir: PathBuf =
            abs_canonical_path(config_dir.join(raw.migrations_dir).as_path())?;
        let cfg: InitialConfig = InitialConfig {
            config_path: absolute_config_path.to_path_buf(),
            dialect: raw.dialect,
            migrations_dir: migrations_dir,
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

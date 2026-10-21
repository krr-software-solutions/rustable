//! Apply rustable migrations to a real database.
//!
//! Applied migrations are recorded in a `rustable_history` table (version, name, checksum,
//! applied-at). Each migration's statements are split with [`rustable_sql_core::statements`] and
//! executed one by one, so MySQL `DELIMITER` blocks and Postgres `DO $$` bodies work without
//! any client-side tricks.
//!
//! Postgres and SQLite run each migration inside a transaction, unless the migration manages
//! its own (`BEGIN`/`COMMIT`) or can't run in one (`CREATE INDEX CONCURRENTLY`). MySQL commits
//! DDL implicitly, so a failed MySQL migration can be left half-applied: the error says so.

#![allow(
    clippy::result_large_err,
    reason = "DbError carries the sqlx::Error that caused it"
)]

use std::path::Path;

use rustable_sql_core::Dialect;
use rustable_sql_core::change::summarize;
use rustable_sql_core::discover::{MigrationFiles, discover};
use rustable_sql_core::statements::{SqlStatement, split_and_parse};
use rustable_sql_core::version::MigrationId;
use sqlx::{AnyConnection, AssertSqlSafe, Connection, Row};

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error(transparent)]
    Core(#[from] rustable_sql_core::Error),
    #[error("{migration} line {line} failed: {sql}\n  {source}{note}")]
    Statement {
        migration: String,
        line: u64,
        sql: String,
        source: sqlx::Error,
        /// Extra context, e.g. "MySQL can't roll back DDL".
        note: String,
    },
    #[error("{version} was applied with a different checksum: its file was edited after it ran")]
    ChecksumMismatch { version: String },
    #[error("{version} is in the history table but its file is missing")]
    MissingFile { version: String },
    #[error(
        "{pending} is pending but older than the already-applied {applied}: refusing to apply out of order"
    )]
    OutOfOrder { pending: String, applied: String },
    #[error("another rustable run held the lock for more than {seconds}s (see --lock-timeout)")]
    LockTimeout { seconds: u64 },
    #[error("lock file {path}: {source}")]
    LockFile {
        path: String,
        source: std::io::Error,
    },
    #[error("{version} is not an applied migration")]
    UnknownTarget { version: String },
    #[error("{migration} has no .undo.sql (nothing was undone)")]
    NoUndo { migration: String },
}

/// One row of `rustable_history`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Applied {
    pub version: String,
    pub name: String,
    pub checksum: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Applied,
    Pending,
    /// Applied, but the file changed since.
    Modified,
    /// In the history table, but no file.
    Missing,
}

#[derive(Clone, Debug)]
pub struct StatusLine {
    pub version: String,
    pub name: String,
    pub state: State,
}

pub struct Database {
    conn: AnyConnection,
    dialect: Dialect,
}

/// A stable 64-bit FNV-1a hash of a migration file, as hex.
#[must_use]
pub fn checksum(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// `V2025_03_14__09_30__name` / `2025_03_14__09_30` → `2025_03_14__09_30`.
fn normalize_version(v: &str) -> String {
    let v = v.trim().trim_start_matches('V');
    v.get(..17).unwrap_or(v).to_string()
}

/// Whether (and how strictly) to use TLS for Postgres/MySQL; ignored for SQLite.
///
/// The names follow Postgres's `sslmode`; for MySQL they map to `ssl-mode` (`VerifyFull` is
/// MySQL's `VERIFY_IDENTITY`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SslMode {
    /// Plain TCP only.
    Disable,
    /// TLS if the server offers it, else plain TCP: sqlx's default for both databases.
    #[default]
    Prefer,
    /// TLS, but don't verify the server's certificate.
    Require,
    /// TLS, and the certificate must chain to a trusted root (`ssl_root_cert`, else the
    /// bundled Mozilla roots).
    VerifyCa,
    /// `VerifyCa`, and the certificate must also match the host name.
    VerifyFull,
}

impl SslMode {
    fn postgres(self) -> &'static str {
        match self {
            SslMode::Disable => "disable",
            SslMode::Prefer => "prefer",
            SslMode::Require => "require",
            SslMode::VerifyCa => "verify-ca",
            SslMode::VerifyFull => "verify-full",
        }
    }

    fn mysql(self) -> &'static str {
        match self {
            SslMode::Disable => "DISABLED",
            SslMode::Prefer => "PREFERRED",
            SslMode::Require => "REQUIRED",
            SslMode::VerifyCa => "VERIFY_CA",
            SslMode::VerifyFull => "VERIFY_IDENTITY",
        }
    }
}

/// Where to connect: separate parts rather than one URL.
#[derive(Clone, Debug, Default)]
pub struct ConnectionParams {
    pub host: String,
    /// Defaults to 5432 (Postgres) / 3306 (MySQL).
    pub port: Option<u16>,
    pub user: Option<String>,
    pub password: Option<String>,
    /// The database name; for SQLite, the database file path.
    pub name: String,
    /// TLS for Postgres/MySQL. `None` leaves sqlx's default (prefer).
    pub ssl_mode: Option<SslMode>,
    /// A PEM file of CA certificates to trust instead of the bundled roots (e.g. a cloud
    /// provider's database CA), for `VerifyCa`/`VerifyFull`.
    pub ssl_root_cert: Option<String>,
}

impl ConnectionParams {
    /// The sqlx connection URL for `dialect`.
    #[must_use]
    pub fn url(&self, dialect: Dialect) -> String {
        match dialect {
            Dialect::Sqlite => format!("sqlite://{}?mode=rwc", self.name),
            Dialect::Postgres | Dialect::MySql => {
                let (scheme, default_port) = if dialect == Dialect::Postgres {
                    ("postgres", 5432)
                } else {
                    ("mysql", 3306)
                };
                let auth = match (&self.user, &self.password) {
                    (Some(u), Some(p)) => format!("{}:{}@", encode(u), encode(p)),
                    (Some(u), None) => format!("{}@", encode(u)),
                    _ => String::new(),
                };
                let (mode_key, cert_key) = if dialect == Dialect::Postgres {
                    ("sslmode", "sslrootcert")
                } else {
                    ("ssl-mode", "ssl-ca")
                };
                let mut query = Vec::new();
                if let Some(mode) = self.ssl_mode {
                    let mode = if dialect == Dialect::Postgres {
                        mode.postgres()
                    } else {
                        mode.mysql()
                    };
                    query.push(format!("{mode_key}={mode}"));
                }
                if let Some(cert) = &self.ssl_root_cert {
                    query.push(format!("{cert_key}={}", encode(cert)));
                }
                let query = if query.is_empty() {
                    String::new()
                } else {
                    format!("?{}", query.join("&"))
                };
                format!(
                    "{scheme}://{auth}{}:{}/{}{query}",
                    self.host,
                    self.port.unwrap_or(default_port),
                    encode(&self.name)
                )
            }
        }
    }
}

/// Percent-encode everything but URL-unreserved characters.
fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// Advisory-lock key for Postgres: "rustable" as bytes.
const LOCK_KEY: i64 = 0x7275_7374_6162_6c65;

/// An exclusive lock on a file (for SQLite), released when dropped or when the process dies.
pub struct FileLock {
    _file: std::fs::File,
}

impl FileLock {
    /// Lock `path` (created if missing), waiting up to `timeout`. `on_wait` is called once if busy.
    ///
    /// # Errors
    /// I/O errors, or [`DbError::LockTimeout`].
    pub async fn acquire(
        path: &Path,
        timeout: std::time::Duration,
        on_wait: impl FnOnce(),
    ) -> Result<FileLock, DbError> {
        let err = |source| DbError::LockFile {
            path: path.display().to_string(),
            source,
        };
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)
            .map_err(err)?;
        let start = std::time::Instant::now();
        let mut on_wait = Some(on_wait);
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(FileLock { _file: file }),
                Err(std::fs::TryLockError::WouldBlock) => {}
                Err(std::fs::TryLockError::Error(e)) => return Err(err(e)),
            }
            if start.elapsed() >= timeout {
                return Err(DbError::LockTimeout {
                    seconds: timeout.as_secs(),
                });
            }
            if let Some(f) = on_wait.take() {
                f();
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
    }
}

/// Current UTC time as `YYYY-MM-DDTHH:MM:SSZ`.
fn utc_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let v = rustable_sql_core::version::Version::from_unix_seconds(secs);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        v.year,
        v.month,
        v.day,
        v.hour,
        v.minute,
        secs % 60
    )
}

fn literal(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn read(path: &Path) -> Result<String, DbError> {
    std::fs::read_to_string(path).map_err(|source| {
        DbError::Core(rustable_sql_core::Error::Io {
            path: path.to_path_buf(),
            source,
        })
    })
}

impl Database {
    /// Connect (rustable's own tables are created later, under the lock: [`Database::ensure_tables`]).
    ///
    /// # Errors
    /// Connection failures.
    pub async fn connect(url: &str, dialect: Dialect) -> Result<Database, DbError> {
        sqlx::any::install_default_drivers();
        let conn = AnyConnection::connect(url).await?;
        let db = Database { conn, dialect };
        Ok(db)
    }

    /// Create rustable's own tables if needed. Call it while holding the lock: concurrent
    /// `CREATE TABLE IF NOT EXISTS` isn't safe on Postgres.
    ///
    /// # Errors
    /// Query failures.
    pub async fn ensure_tables(&mut self) -> Result<(), DbError> {
        let dialect = self.dialect;
        self.exec(
            "CREATE TABLE IF NOT EXISTS rustable_history (
                version VARCHAR(17) NOT NULL PRIMARY KEY,
                name VARCHAR(255) NOT NULL,
                checksum VARCHAR(16) NOT NULL,
                applied_at VARCHAR(32) NOT NULL
            )",
        )
        .await?;
        let id_column = match dialect {
            Dialect::Postgres => "id BIGSERIAL PRIMARY KEY",
            Dialect::MySql => "id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY",
            Dialect::Sqlite => "id INTEGER PRIMARY KEY AUTOINCREMENT",
        };
        self.exec(&format!(
            "CREATE TABLE IF NOT EXISTS rustable_events (
                {id_column},
                at VARCHAR(32) NOT NULL,
                event VARCHAR(32) NOT NULL,
                version VARCHAR(17) NOT NULL,
                detail TEXT
            )"
        ))
        .await?;
        Ok(())
    }

    /// Take the database-wide rustable lock (Postgres advisory lock / MySQL `GET_LOCK`), waiting
    /// up to `timeout`. `on_wait` is called once if the lock is busy. SQLite has no such lock;
    /// use [`FileLock`] instead.
    ///
    /// # Errors
    /// [`DbError::LockTimeout`] if another run holds the lock for longer than `timeout`.
    pub async fn lock(
        &mut self,
        timeout: std::time::Duration,
        on_wait: impl FnOnce(),
    ) -> Result<(), DbError> {
        match self.dialect {
            Dialect::Postgres => {
                let start = std::time::Instant::now();
                let mut on_wait = Some(on_wait);
                loop {
                    let got: bool = sqlx::query(AssertSqlSafe(format!(
                        "SELECT pg_try_advisory_lock({LOCK_KEY})"
                    )))
                    .fetch_one(&mut self.conn)
                    .await?
                    .try_get(0)?;
                    if got {
                        return Ok(());
                    }
                    if start.elapsed() >= timeout {
                        return Err(DbError::LockTimeout {
                            seconds: timeout.as_secs(),
                        });
                    }
                    if let Some(f) = on_wait.take() {
                        f();
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
            Dialect::MySql => {
                // GET_LOCK waits server-side; ask cheaply first so we can report the wait.
                if self.mysql_get_lock(0).await? {
                    return Ok(());
                }
                on_wait();
                if self.mysql_get_lock(timeout.as_secs()).await? {
                    Ok(())
                } else {
                    Err(DbError::LockTimeout {
                        seconds: timeout.as_secs(),
                    })
                }
            }
            Dialect::Sqlite => Ok(()),
        }
    }

    async fn mysql_get_lock(&mut self, seconds: u64) -> Result<bool, DbError> {
        let row = sqlx::query(AssertSqlSafe(format!(
            "SELECT GET_LOCK(CONCAT('rustable:', DATABASE()), {seconds})"
        )))
        .fetch_one(&mut self.conn)
        .await?;
        let got: Option<i64> = row.try_get(0)?;
        Ok(got == Some(1))
    }

    /// Release the lock taken by [`Database::lock`]. (Closing the connection releases it too.)
    ///
    /// # Errors
    /// Query failures.
    pub async fn unlock(&mut self) -> Result<(), DbError> {
        let sql = match self.dialect {
            Dialect::Postgres => format!("SELECT pg_advisory_unlock({LOCK_KEY})"),
            Dialect::MySql => "SELECT RELEASE_LOCK(CONCAT('rustable:', DATABASE()))".to_string(),
            Dialect::Sqlite => return Ok(()),
        };
        sqlx::query(AssertSqlSafe(sql))
            .fetch_one(&mut self.conn)
            .await?;
        Ok(())
    }

    /// Append to `rustable_events` (audit only; never read back).
    async fn event(
        &mut self,
        event: &str,
        version: &str,
        detail: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        self.exec(&format!(
            "INSERT INTO rustable_events (at, event, version, detail) VALUES ({}, {}, {}, {})",
            literal(&utc_now()),
            literal(event),
            literal(version),
            detail.map_or_else(|| "NULL".to_string(), literal)
        ))
        .await
    }

    async fn exec(&mut self, sql: &str) -> Result<(), sqlx::Error> {
        sqlx::raw_sql(AssertSqlSafe(sql.to_string()))
            .execute(&mut self.conn)
            .await
            .map(|_| ())
    }

    /// Everything in `rustable_history`, oldest first.
    ///
    /// # Errors
    /// Query failures.
    pub async fn applied(&mut self) -> Result<Vec<Applied>, DbError> {
        let rows = sqlx::query(AssertSqlSafe(
            "SELECT version, name, checksum FROM rustable_history ORDER BY version".to_string(),
        ))
        .fetch_all(&mut self.conn)
        .await?;
        rows.iter()
            .map(|row| {
                Ok(Applied {
                    version: row.try_get::<String, _>(0)?,
                    name: row.try_get::<String, _>(1)?,
                    checksum: row.try_get::<String, _>(2)?,
                })
            })
            .collect::<Result<_, sqlx::Error>>()
            .map_err(DbError::from)
    }

    /// Compare the migrations directory with the history table.
    ///
    /// # Errors
    /// Discovery or query failures.
    pub async fn status(&mut self, dir: &Path) -> Result<Vec<StatusLine>, DbError> {
        let files = discover(dir)?;
        let applied = self.applied().await?;
        let mut lines = Vec::new();
        for f in &files {
            let version = f.id.version.to_string();
            let state = match applied.iter().find(|a| a.version == version) {
                None => State::Pending,
                Some(a) if a.checksum == checksum(&read(&f.up)?) => State::Applied,
                Some(_) => State::Modified,
            };
            lines.push(StatusLine {
                version,
                name: f.id.name.clone(),
                state,
            });
        }
        for a in &applied {
            if !files.iter().any(|f| f.id.version.to_string() == a.version) {
                lines.push(StatusLine {
                    version: a.version.clone(),
                    name: a.name.clone(),
                    state: State::Missing,
                });
            }
        }
        lines.sort_by(|a, b| a.version.cmp(&b.version));
        Ok(lines)
    }

    /// Apply every pending migration (up to and including `target`, if given).
    ///
    /// # Errors
    /// Edited or missing applied migrations, out-of-order pending ones, or a failing statement.
    pub async fn migrate(
        &mut self,
        dir: &Path,
        target: Option<&str>,
    ) -> Result<Vec<MigrationId>, DbError> {
        let files = discover(dir)?;
        let applied = self.applied().await?;

        for a in &applied {
            let Some(f) = files.iter().find(|f| f.id.version.to_string() == a.version) else {
                return Err(DbError::MissingFile {
                    version: a.version.clone(),
                });
            };
            if checksum(&read(&f.up)?) != a.checksum {
                return Err(DbError::ChecksumMismatch {
                    version: f.id.to_string(),
                });
            }
        }
        let latest = applied.iter().map(|a| a.version.clone()).max();

        let mut done = Vec::new();
        for f in &files {
            let version = f.id.version.to_string();
            if applied.iter().any(|a| a.version == version) {
                continue;
            }
            if target.is_some_and(|t| version.as_str() > t) {
                break;
            }
            if let Some(latest) = &latest
                && version < *latest
            {
                return Err(DbError::OutOfOrder {
                    pending: f.id.to_string(),
                    applied: latest.clone(),
                });
            }
            let text = read(&f.up)?;
            let record = format!(
                "INSERT INTO rustable_history (version, name, checksum, applied_at) VALUES ({}, {}, {}, {})",
                literal(&version),
                literal(&f.id.name),
                literal(&checksum(&text)),
                literal(&now_unix())
            );
            self.run_script("migration", &f.id, &text, &record).await?;
            done.push(f.id.clone());
        }
        Ok(done)
    }

    /// Undo every applied migration newer than `target` (newest first), using `.undo.sql` files.
    /// `target` stays applied. It may be a version (`2025_03_14__09_30`) or a full stem
    /// (`V2025_03_14__09_30__create-notes`).
    ///
    /// Everything is checked up front: if any migration in range has no undo file, or `target`
    /// isn't applied, nothing runs.
    ///
    /// # Errors
    /// An unknown target, a missing undo file, or a failing statement.
    pub async fn undo_to(&mut self, dir: &Path, target: &str) -> Result<Vec<MigrationId>, DbError> {
        let target = normalize_version(target);
        let files = discover(dir)?;
        let applied = self.applied().await?;
        if !applied.iter().any(|a| a.version == target) {
            return Err(DbError::UnknownTarget { version: target });
        }

        let mut plan: Vec<(&MigrationFiles, String)> = Vec::new();
        for a in applied.iter().rev().filter(|a| a.version > target) {
            let f = files
                .iter()
                .find(|f| f.id.version.to_string() == a.version)
                .ok_or_else(|| DbError::MissingFile {
                    version: a.version.clone(),
                })?;
            let undo = f.undo.as_ref().ok_or_else(|| DbError::NoUndo {
                migration: f.id.to_string(),
            })?;
            plan.push((f, read(undo)?));
        }

        let mut done = Vec::new();
        for (f, text) in plan {
            let record = format!(
                "DELETE FROM rustable_history WHERE version = {}",
                literal(&f.id.version.to_string())
            );
            self.run_script("undo", &f.id, &text, &record).await?;
            done.push(f.id.clone());
        }
        Ok(done)
    }

    /// Run a script's statements, then `record` (the history update), in a transaction if possible.
    async fn run_script(
        &mut self,
        kind: &str,
        id: &MigrationId,
        text: &str,
        record: &str,
    ) -> Result<(), DbError> {
        let version = id.version.to_string();
        // Committed on its own, before the migration's transaction: survives a crash or rollback.
        self.event(&format!("{kind}_started"), &version, None)
            .await?;
        let result = self.run_statements(id, text, record).await;
        match &result {
            Ok(()) => {
                self.event(&format!("{kind}_succeeded"), &version, None)
                    .await?;
            }
            Err(e) => {
                self.event(&format!("{kind}_failed"), &version, Some(&e.to_string()))
                    .await?;
            }
        }
        result
    }

    async fn run_statements(
        &mut self,
        id: &MigrationId,
        text: &str,
        record: &str,
    ) -> Result<(), DbError> {
        let statements = split_and_parse(text, self.dialect);
        let transactional = self.dialect != Dialect::MySql && !manages_own_transaction(&statements);
        if transactional {
            self.exec("BEGIN").await?;
        }
        for stmt in &statements {
            if let Err(source) = self.exec(&stmt.sql).await {
                if transactional {
                    let _ = self.exec("ROLLBACK").await;
                }
                return Err(DbError::Statement {
                    migration: id.to_string(),
                    line: stmt.line,
                    sql: summarize(&stmt.sql),
                    source,
                    note: if transactional {
                        "\n  (rolled back)".to_string()
                    } else {
                        "\n  (not in a transaction: earlier statements of this migration may have been applied)".to_string()
                    },
                });
            }
        }
        if let Err(e) = self.exec(record).await {
            if transactional {
                let _ = self.exec("ROLLBACK").await;
            }
            return Err(e.into());
        }
        if transactional {
            self.exec("COMMIT").await?;
        }
        Ok(())
    }
}

impl Database {
    /// The tables and columns that really exist (name, type, nullability only).
    ///
    /// # Errors
    /// Query failures.
    pub async fn introspect(&mut self) -> Result<Vec<(String, String, String, bool)>, DbError> {
        let sql = match self.dialect {
            Dialect::Postgres => {
                "SELECT CASE WHEN n.nspname = 'public' THEN c.relname::text ELSE n.nspname || '.' || c.relname END,
                        a.attname::text, format_type(a.atttypid, a.atttypmod), a.attnotnull
                   FROM pg_attribute a
                   JOIN pg_class c ON a.attrelid = c.oid
                   JOIN pg_namespace n ON c.relnamespace = n.oid
                  WHERE c.relkind IN ('r', 'p') AND NOT c.relispartition
                    AND a.attnum > 0 AND NOT a.attisdropped
                    AND n.nspname NOT IN ('pg_catalog', 'information_schema') AND n.nspname NOT LIKE 'pg_toast%'
                  ORDER BY 1, a.attnum"
            }
            Dialect::MySql => {
                "SELECT CAST(c.TABLE_NAME AS CHAR), CAST(c.COLUMN_NAME AS CHAR), CAST(c.COLUMN_TYPE AS CHAR),
                        c.IS_NULLABLE = 'NO'
                   FROM information_schema.COLUMNS c
                   JOIN information_schema.TABLES t ON t.TABLE_SCHEMA = c.TABLE_SCHEMA AND t.TABLE_NAME = c.TABLE_NAME
                  WHERE c.TABLE_SCHEMA = DATABASE() AND t.TABLE_TYPE = 'BASE TABLE'
                  ORDER BY c.TABLE_NAME, c.ORDINAL_POSITION"
            }
            Dialect::Sqlite => {
                "SELECT m.name, p.name, p.type, (p.\"notnull\" = 1 OR p.pk > 0)
                   FROM sqlite_master m JOIN pragma_table_info(m.name) p
                  WHERE m.type = 'table' AND m.name NOT LIKE 'sqlite_%'
                    AND m.sql NOT LIKE 'CREATE VIRTUAL TABLE%'
                    AND NOT EXISTS (SELECT 1 FROM sqlite_master v
                                     WHERE v.sql LIKE 'CREATE VIRTUAL TABLE%' AND m.name LIKE v.name || '_%')
                  ORDER BY m.name, p.cid"
            }
        };
        let rows = sqlx::query(AssertSqlSafe(sql.to_string()))
            .fetch_all(&mut self.conn)
            .await?;
        rows.iter()
            .map(|row| {
                Ok((
                    row.try_get::<String, _>(0)?,
                    row.try_get::<String, _>(1)?,
                    row.try_get::<String, _>(2)?,
                    // Postgres returns a bool; MySQL and SQLite an integer.
                    row.try_get::<bool, _>(3)
                        .or_else(|_| row.try_get::<i64, _>(3).map(|n| n != 0))?,
                ))
            })
            .filter(|r: &Result<(String, String, String, bool), sqlx::Error>| {
                r.as_ref().map_or(true, |(t, ..)| t != "rustable_history")
            })
            .collect::<Result<_, sqlx::Error>>()
            .map_err(DbError::from)
    }

    /// `(table, index name)` for every real index (excluding primary keys and SQLite auto-indexes).
    ///
    /// # Errors
    /// Query failures.
    pub async fn introspect_indexes(&mut self) -> Result<Vec<(String, String)>, DbError> {
        let sql = match self.dialect {
            Dialect::Postgres => {
                "SELECT CASE WHEN i.schemaname = 'public' THEN i.tablename::text ELSE i.schemaname || '.' || i.tablename END,
                        i.indexname::text
                   FROM pg_indexes i
                   JOIN pg_class ic ON ic.relname = i.indexname
                   JOIN pg_index x ON x.indexrelid = ic.oid
                  WHERE i.schemaname NOT IN ('pg_catalog', 'information_schema') AND NOT x.indisprimary
                  ORDER BY 1, 2"
            }
            Dialect::MySql => {
                "SELECT DISTINCT CAST(TABLE_NAME AS CHAR), CAST(INDEX_NAME AS CHAR)
                   FROM information_schema.STATISTICS
                  WHERE TABLE_SCHEMA = DATABASE() AND INDEX_NAME <> 'PRIMARY'
                  ORDER BY 1, 2"
            }
            Dialect::Sqlite => {
                "SELECT tbl_name, name FROM sqlite_master
                  WHERE type = 'index' AND sql IS NOT NULL ORDER BY 1, 2"
            }
        };
        let rows = sqlx::query(AssertSqlSafe(sql.to_string()))
            .fetch_all(&mut self.conn)
            .await?;
        rows.iter()
            .map(|row| Ok((row.try_get::<String, _>(0)?, row.try_get::<String, _>(1)?)))
            .collect::<Result<_, sqlx::Error>>()
            .map_err(DbError::from)
    }

    /// Compare the real database with the schema rustable derives from the migrations.
    /// Returns the differences (empty = they agree).
    ///
    /// # Errors
    /// Analysis or query failures.
    pub async fn verify(&mut self, dir: &Path) -> Result<Vec<String>, DbError> {
        let applied: Vec<String> = self
            .applied()
            .await?
            .into_iter()
            .map(|a| a.version)
            .collect();
        let mut sources = Vec::new();
        for f in discover(dir)? {
            if applied.contains(&f.id.version.to_string()) {
                sources.push(rustable_sql_core::project::MigrationSource {
                    up: read(&f.up)?,
                    undo: None,
                    id: f.id,
                    up_path: None,
                    undo_path: None,
                });
            }
        }
        let expected = rustable_sql_core::project::analyze(sources, self.dialect).schema;
        let actual = self.introspect().await?;
        let mut differences = compare(&expected, &actual, self.dialect);
        let indexes = self.introspect_indexes().await?;
        differences.extend(compare_indexes(&expected, &indexes, self.dialect));
        Ok(differences)
    }
}

fn compare(
    expected: &rustable_sql_core::schema::Schema,
    actual: &[(String, String, String, bool)],
    dialect: Dialect,
) -> Vec<String> {
    let mut out = Vec::new();
    let tables: Vec<&rustable_sql_core::schema::Table> = expected
        .tables
        .iter()
        .filter(|t| !t.options.contains_key("PARTITION OF"))
        .collect();
    let actual_tables: std::collections::BTreeSet<&str> =
        actual.iter().map(|(t, ..)| t.as_str()).collect();
    for t in &tables {
        if !actual_tables.contains(t.name.as_str()) {
            out.push(format!(
                "table `{}` expected but not in the database",
                t.name
            ));
        }
    }
    for name in &actual_tables {
        let Some(table) = tables.iter().find(|t| t.name == *name) else {
            out.push(format!(
                "table `{name}` is in the database but not in rustable's schema"
            ));
            continue;
        };
        let columns: Vec<_> = actual.iter().filter(|(t, ..)| t == name).collect();
        for (_, col, db_type, not_null) in &columns {
            match table.column(col) {
                None => out.push(format!(
                    "column `{name}.{col}` is in the database but not in rustable's schema"
                )),
                Some(c) => {
                    let (a, b) = (
                        comparable_type(&c.data_type, dialect),
                        comparable_type(db_type, dialect),
                    );
                    if a != b {
                        out.push(format!(
                            "column `{name}.{col}`: rustable says {}, database says {db_type}",
                            c.data_type
                        ));
                    }
                    if c.nullable == *not_null {
                        out.push(format!(
                            "column `{name}.{col}`: rustable says {}, database says {}",
                            if c.nullable { "nullable" } else { "NOT NULL" },
                            if *not_null { "NOT NULL" } else { "nullable" }
                        ));
                    }
                }
            }
        }
        for c in &table.columns {
            if !columns.iter().any(|(_, col, ..)| *col == c.name) {
                out.push(format!(
                    "column `{name}.{}` expected but not in the database",
                    c.name
                ));
            }
        }
    }
    out
}

fn compare_indexes(
    expected: &rustable_sql_core::schema::Schema,
    actual: &[(String, String)],
    dialect: Dialect,
) -> Vec<String> {
    let mut out = Vec::new();
    for table in expected
        .tables
        .iter()
        .filter(|t| !t.options.contains_key("PARTITION OF"))
    {
        let mut want: Vec<&str> = table
            .indexes
            .iter()
            .filter_map(|i| i.name.as_deref())
            .collect();
        want.sort_unstable();
        want.dedup();
        let fk_names: Vec<&str> = table
            .foreign_keys
            .iter()
            .filter_map(|f| f.name.as_deref())
            .collect();
        let have: Vec<&str> = actual
            .iter()
            .filter(|(t, _)| *t == table.name)
            .map(|(_, i)| i.as_str())
            // MySQL creates an index for each foreign key, named after it.
            .filter(|i| !(dialect == Dialect::MySql && fk_names.contains(i) && !want.contains(i)))
            .collect();
        for name in &want {
            if !have.contains(name) {
                out.push(format!(
                    "index `{name}` on `{}` expected but not in the database",
                    table.name
                ));
            }
        }
        for name in &have {
            if !want.contains(name) {
                out.push(format!(
                    "index `{name}` on `{}` is in the database but not in rustable's schema",
                    table.name
                ));
            }
        }
    }
    out
}

fn comparable_type(t: &str, dialect: Dialect) -> String {
    let canonical = rustable_sql_core::types::canonical_type(t, dialect).replace(", ", ",");
    match canonical.as_str() {
        "SERIAL" => "INTEGER".to_string(),
        "BIGSERIAL" => "BIGINT".to_string(),
        "SMALLSERIAL" => "SMALLINT".to_string(),
        _ => canonical,
    }
}

/// Scripts with their own `BEGIN`/`COMMIT`, or `CREATE INDEX CONCURRENTLY` (which can't run in a
/// transaction), are run as-is.
fn manages_own_transaction(statements: &[SqlStatement]) -> bool {
    statements.iter().any(|s| {
        let kw = s.leading_keyword();
        matches!(kw.as_str(), "BEGIN" | "START" | "COMMIT" | "END")
            || s.sql.to_ascii_uppercase().contains("CONCURRENTLY")
    })
}

fn now_unix() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_urls() {
        let p = ConnectionParams {
            host: "db.local".into(),
            port: None,
            user: Some("app".into()),
            password: Some("p@ss:w/rd".into()),
            name: "shop".into(),
            ..ConnectionParams::default()
        };
        assert_eq!(
            p.url(Dialect::Postgres),
            "postgres://app:p%40ss%3Aw%2Frd@db.local:5432/shop"
        );
        assert_eq!(
            p.url(Dialect::MySql),
            "mysql://app:p%40ss%3Aw%2Frd@db.local:3306/shop"
        );
        let lite = ConnectionParams {
            name: "/tmp/x.db".into(),
            ..ConnectionParams::default()
        };
        assert_eq!(lite.url(Dialect::Sqlite), "sqlite:///tmp/x.db?mode=rwc");
    }

    #[test]
    fn builds_tls_urls() {
        let p = ConnectionParams {
            host: "db.local".into(),
            user: Some("app".into()),
            name: "shop".into(),
            ssl_mode: Some(SslMode::VerifyFull),
            ssl_root_cert: Some("/certs/rds ca.pem".into()),
            ..ConnectionParams::default()
        };
        assert_eq!(
            p.url(Dialect::Postgres),
            "postgres://app@db.local:5432/shop?sslmode=verify-full&sslrootcert=%2Fcerts%2Frds%20ca.pem"
        );
        assert_eq!(
            p.url(Dialect::MySql),
            "mysql://app@db.local:3306/shop?ssl-mode=VERIFY_IDENTITY&ssl-ca=%2Fcerts%2Frds%20ca.pem"
        );
        // TLS settings don't apply to SQLite.
        assert_eq!(
            ConnectionParams {
                name: "x.db".into(),
                ssl_mode: Some(SslMode::Require),
                ..ConnectionParams::default()
            }
            .url(Dialect::Sqlite),
            "sqlite://x.db?mode=rwc"
        );
    }

    #[test]
    fn normalizes_versions() {
        assert_eq!(
            normalize_version("V2025_03_14__09_30__create-notes"),
            "2025_03_14__09_30"
        );
        assert_eq!(normalize_version("2025_03_14__09_30"), "2025_03_14__09_30");
    }
}

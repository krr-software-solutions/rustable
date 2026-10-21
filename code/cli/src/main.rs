use std::path::{Path, PathBuf};

use crate::config::InitialConfig;
use crate::error::RustableError;
use clap::{Args, CommandFactory, Parser, Subcommand};
use rustable_sql_core::change::Change;
use rustable_sql_core::project::{Project, analyze_dir};
use rustable_sql_core::undo::GeneratedUndo;

pub mod config;
pub mod error;

//

/// SQL schema migrations, in Rust.
#[derive(Debug, Parser)]
#[command(name = "rustable", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Args)]
struct ConfigArgs {
    /// Path to the project's config.toml
    #[arg(long, env = "RUSTABLE_CONFIG_PATH")]
    config_path: PathBuf,
}

#[derive(Debug, Subcommand)]
enum Command {
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Analyze every migration: reversibility, compatibility, undo checks
    Analyze {
        #[command(flatten)]
        config: ConfigArgs,
    },
    /// Write schema.toml, MIGRATIONS.md, and trivial .undo.sql files
    Generate {
        #[command(flatten)]
        config: ConfigArgs,
        /// Don't write auto-generated .undo.sql files
        #[arg(long)]
        no_undo: bool,
    },
    /// Verify schema.toml / MIGRATIONS.md are up to date and every .undo.sql restores the schema
    Check {
        #[command(flatten)]
        config: ConfigArgs,
    },
    /// Show how the code's tables (code.current_schema.toml) differ from the migrations
    Diff {
        #[command(flatten)]
        config: ConfigArgs,
    },
    /// Plan a migration that brings the migrations in line with the code's tables
    Plan {
        #[command(flatten)]
        config: ConfigArgs,
        /// Never drop: make removed columns nullable / leave removed tables, and report them
        #[arg(long)]
        soft_drops: bool,
        /// Write the migration (and the outstanding report) instead of printing it
        #[arg(long)]
        write: bool,
        /// Name for the new migration
        #[arg(long, default_value = "sync-code-schema")]
        name: String,
    },
    /// Apply migrations to a real database
    Db {
        #[command(subcommand)]
        command: DbCommand,
    },
    /// Print a shell completion script
    Completions { shell: clap_complete::Shell },
}

#[derive(Debug, Args)]
struct DbArgs {
    #[command(flatten)]
    config: ConfigArgs,
    #[arg(long, env = "RUSTABLE_DB_HOST", default_value = "127.0.0.1")]
    db_host: String,
    /// Default: 5432 for Postgres, 3306 for MySQL
    #[arg(long, env = "RUSTABLE_DB_PORT")]
    db_port: Option<u16>,
    #[arg(long, env = "RUSTABLE_DB_USER")]
    db_user: Option<String>,
    #[arg(long, env = "RUSTABLE_DB_PASSWORD", hide_env_values = true)]
    db_password: Option<String>,
    /// Database name (for SQLite: the database file path)
    #[arg(long, env = "RUSTABLE_DB_NAME")]
    db_name: String,
    /// TLS for Postgres/MySQL [default: prefer: TLS if the server offers it]
    #[arg(long, env = "RUSTABLE_DB_SSL_MODE", value_enum)]
    db_ssl_mode: Option<SslModeArg>,
    /// PEM file of CA certificates to trust for verify-ca/verify-full, instead of the bundled
    /// Mozilla roots (e.g. your cloud provider's database CA bundle)
    #[arg(long, env = "RUSTABLE_DB_SSL_ROOT_CERT")]
    db_ssl_root_cert: Option<PathBuf>,
    /// How long to wait for another rustable run to finish (seconds)
    #[arg(long, env = "RUSTABLE_LOCK_TIMEOUT", default_value_t = 300)]
    lock_timeout: u64,
    /// SQLite only: lock this file while running, so concurrent runs wait for each other
    #[arg(long, env = "RUSTABLE_LOCK_FILE")]
    lock_file: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum SslModeArg {
    Disable,
    Prefer,
    Require,
    VerifyCa,
    VerifyFull,
}

impl From<SslModeArg> for rustable_sql_db::SslMode {
    fn from(mode: SslModeArg) -> Self {
        match mode {
            SslModeArg::Disable => Self::Disable,
            SslModeArg::Prefer => Self::Prefer,
            SslModeArg::Require => Self::Require,
            SslModeArg::VerifyCa => Self::VerifyCa,
            SslModeArg::VerifyFull => Self::VerifyFull,
        }
    }
}

#[derive(Debug, Subcommand)]
enum DbCommand {
    /// Show which migrations are applied, pending, edited or missing
    Status {
        #[command(flatten)]
        db: DbArgs,
    },
    /// Apply pending migrations
    Migrate {
        #[command(flatten)]
        db: DbArgs,
        /// Stop after this version (e.g. 2025_03_14__09_30)
        #[arg(long)]
        to: Option<String>,
    },
    /// Compare the real database's tables and columns with rustable's derived schema
    Verify {
        #[command(flatten)]
        db: DbArgs,
    },
    /// Undo applied migrations back to a version, with their .undo.sql files
    Undo {
        #[command(flatten)]
        db: DbArgs,
        /// Undo everything applied after this version (which stays applied)
        #[arg(long)]
        to: String,
    },
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    Show {
        #[command(flatten)]
        config: ConfigArgs,
    },
}

//

async fn command_config_show(config_path: PathBuf) -> Result<(), RustableError> {
    let config: crate::config::InitialConfig =
        crate::config::InitialConfig::load(&config_path).await?;
    println!("Parsed Config:\n{:#?}", config);
    Ok(())
}

async fn load_project(config_path: &Path) -> Result<(InitialConfig, Project), RustableError> {
    let config: InitialConfig = InitialConfig::load(config_path).await?;
    let project: Project = analyze_dir(&config.migrations_dir_path, config.dialect)?;
    Ok((config, project))
}

const MIGRATIONS_MD_TITLE: &str = "Migrations";

async fn command_analyze(config_path: PathBuf) -> Result<(), RustableError> {
    let (_, project) = load_project(&config_path).await?;
    for m in &project.migrations {
        println!("{}", m.id);
        println!("  deploy:     {}", m.phase);
        println!(
            "  reversible: {}{}",
            m.reversibility.label(),
            m.reversibility
                .reason()
                .map(|r| format!(" ({r})"))
                .unwrap_or_default()
        );
        println!("  backward:   {}", describe(&m.backward));
        println!("  forward:    {}", describe(&m.forward));
        let undo = match (&m.provided_undo, &m.generated_undo) {
            (Some(check), _) if check.restores_schema() => {
                format!("provided ✓ (undo itself: {})", check.reversibility.label())
            }
            (Some(check), _) => format!("provided ✗ {}", check.differences.join("; ")),
            (None, GeneratedUndo::Generated(_)) => "missing, can be generated".to_string(),
            (None, GeneratedUndo::NotTrivial(reasons)) => {
                format!("missing, not trivial ({} reason(s))", reasons.len())
            }
        };
        println!("  undo:       {undo}");
        for step in &m.steps {
            if let Change::Unknown { reason, .. } = &step.change {
                println!("  ? line {}: {reason}", step.line);
            }
        }
        println!();
    }
    Ok(())
}

fn describe(c: &rustable_sql_core::analysis::Compat) -> String {
    match c.reason() {
        Some(r) => format!("{} ({r})", c.label()),
        None => c.label().to_string(),
    }
}

async fn write_file(path: &Path, contents: &str) -> Result<(), RustableError> {
    tokio::fs::write(path, contents)
        .await
        .map_err(|source| RustableError::Write {
            path: path.to_path_buf(),
            source,
        })
}

async fn command_generate(config_path: PathBuf, no_undo: bool) -> Result<(), RustableError> {
    let (config, mut project) = load_project(&config_path).await?;

    // Undo files first: they change what the report says, so re-analyze after writing them.
    if !no_undo {
        let mut wrote_any = false;
        for m in &project.migrations {
            if let (None, GeneratedUndo::Generated(script)) = (&m.undo_path, &m.generated_undo) {
                let path = config.migrations_dir_path.join(m.id.undo_file_name());
                write_file(&path, script).await?;
                println!("wrote {}", path.display());
                wrote_any = true;
            }
        }
        if wrote_any {
            project = analyze_dir(&config.migrations_dir_path, config.dialect)?;
        }
    }

    write_file(&config.current_schema_path, &project.schema.to_toml()?).await?;
    println!("wrote {}", config.current_schema_path.display());

    let md = rustable_sql_core::report::migrations_md(&project, MIGRATIONS_MD_TITLE);
    write_file(&config.migrations_md_path, &md).await?;
    println!("wrote {}", config.migrations_md_path.display());
    Ok(())
}

async fn command_check(config_path: PathBuf) -> Result<(), RustableError> {
    let (config, project) = load_project(&config_path).await?;
    let mut problems: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    let expected_schema = project.schema.to_toml()?;
    match tokio::fs::read_to_string(&config.current_schema_path).await {
        Ok(actual) if actual == expected_schema => {}
        Ok(_) => problems.push(format!(
            "{} is out of date (run `rustable generate`)",
            config.current_schema_path.display()
        )),
        Err(_) => problems.push(format!(
            "{} is missing",
            config.current_schema_path.display()
        )),
    }
    let expected_md = rustable_sql_core::report::migrations_md(&project, MIGRATIONS_MD_TITLE);
    match tokio::fs::read_to_string(&config.migrations_md_path).await {
        Ok(actual) if actual == expected_md => {}
        Ok(_) => problems.push(format!(
            "{} is out of date (run `rustable generate`)",
            config.migrations_md_path.display()
        )),
        Err(_) => problems.push(format!(
            "{} is missing",
            config.migrations_md_path.display()
        )),
    }

    for m in &project.migrations {
        if let Some(check) = &m.provided_undo
            && !check.restores_schema()
        {
            problems.push(format!(
                "{}: the undo does not restore the schema: {}",
                m.id.undo_file_name(),
                check.differences.join("; ")
            ));
        }
        if let (None, GeneratedUndo::Generated(_)) = (&m.provided_undo, &m.generated_undo) {
            warnings.push(format!("{}: no undo, but one can be generated", m.id));
        }
        for step in &m.steps {
            if let Change::Unknown { reason, .. } = &step.change {
                warnings.push(format!("{} line {}: {reason}", m.id, step.line));
            }
        }
    }

    for w in &warnings {
        println!("warning: {w}");
    }
    for p in &problems {
        println!("problem: {p}");
    }
    if problems.is_empty() {
        println!("ok: {} migrations checked", project.migrations.len());
        Ok(())
    } else {
        Err(RustableError::CheckFailed {
            problems: problems.len(),
        })
    }
}

async fn load_code_schema(
    config: &InitialConfig,
) -> Result<rustable_sql_core::schema::Schema, RustableError> {
    let text = tokio::fs::read_to_string(&config.code_schema_path)
        .await
        .map_err(|source| RustableError::ConfigRead {
            path: config.code_schema_path.clone(),
            source,
        })?;
    toml::from_str(&text).map_err(|source| RustableError::ConfigParse {
        path: config.code_schema_path.clone(),
        source,
    })
}

async fn command_diff(config_path: PathBuf) -> Result<(), RustableError> {
    let (config, project) = load_project(&config_path).await?;
    let code = load_code_schema(&config).await?;
    let differences = rustable_sql_core::diff::diff(&project.schema, &code);
    if differences.is_empty() {
        println!("the code's tables match the migrations");
    }
    for d in differences {
        println!("{d}");
    }
    Ok(())
}

async fn command_plan(
    config_path: PathBuf,
    soft_drops: bool,
    write: bool,
    name: String,
) -> Result<(), RustableError> {
    let (config, project) = load_project(&config_path).await?;
    let code = load_code_schema(&config).await?;
    let plan = rustable_sql_core::plan::plan(
        &project.schema,
        &code,
        config.dialect,
        rustable_sql_core::plan::PlanOptions { soft_drops },
    );
    if plan.is_empty() && plan.outstanding.is_empty() {
        println!("nothing to do: the code's tables match the migrations");
        return Ok(());
    }
    if !write {
        print!("{}", plan.script());
        return Ok(());
    }

    if !plan.is_empty() {
        let id = rustable_sql_core::version::MigrationId {
            version: rustable_sql_core::version::Version::now(),
            name,
        };
        let path = config.migrations_dir_path.join(id.up_file_name());
        write_file(&path, &plan.script()).await?;
        println!("wrote {}", path.display());
    }
    let mut report = String::from(
        "# Outstanding differences\n\n_Written by `rustable plan`: differences between the code's tables and the migrations that were deliberately not applied._\n\n",
    );
    for o in &plan.outstanding {
        report.push_str(&format!("- [ ] {o}\n"));
    }
    if plan.outstanding.is_empty() {
        report.push_str("_None._\n");
    }
    write_file(&config.outstanding_path, &report).await?;
    println!("wrote {}", config.outstanding_path.display());
    Ok(())
}

async fn connect(db: &DbArgs) -> Result<(InitialConfig, rustable_sql_db::Database), RustableError> {
    let config: InitialConfig = InitialConfig::load(&db.config.config_path).await?;
    let params = rustable_sql_db::ConnectionParams {
        host: db.db_host.clone(),
        port: db.db_port,
        user: db.db_user.clone(),
        password: db.db_password.clone(),
        name: db.db_name.clone(),
        ssl_mode: db.db_ssl_mode.map(Into::into),
        ssl_root_cert: db
            .db_ssl_root_cert
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned()),
    };
    let database =
        rustable_sql_db::Database::connect(&params.url(config.dialect), config.dialect).await?;
    Ok((config, database))
}

/// A connection holding the rustable lock: the database lock (Postgres/MySQL) or, for SQLite,
/// the optional lock file. Concurrent runs (e.g. app instances migrating on startup) wait here.
async fn connect_locked(
    db: &DbArgs,
) -> Result<
    (
        InitialConfig,
        rustable_sql_db::Database,
        Option<rustable_sql_db::FileLock>,
    ),
    RustableError,
> {
    let timeout = std::time::Duration::from_secs(db.lock_timeout);
    let waiting = || eprintln!("waiting for another rustable run to finish (lock held)…");
    let file_lock = match &db.lock_file {
        Some(path) => Some(rustable_sql_db::FileLock::acquire(path, timeout, waiting).await?),
        None => None,
    };
    let (config, mut database) = connect(db).await?;
    if file_lock.is_none() {
        database.lock(timeout, waiting).await?;
    }
    database.ensure_tables().await?;
    Ok((config, database, file_lock))
}

async fn command_db(command: DbCommand) -> Result<(), RustableError> {
    match command {
        DbCommand::Status { db } => {
            let (config, mut database, _lock) = connect_locked(&db).await?;
            for line in database.status(&config.migrations_dir_path).await? {
                let state = match line.state {
                    rustable_sql_db::State::Applied => "applied",
                    rustable_sql_db::State::Pending => "pending",
                    rustable_sql_db::State::Modified => "EDITED SINCE APPLIED",
                    rustable_sql_db::State::Missing => "MISSING FILE",
                };
                println!("{:<20} {:<17} {}", state, line.version, line.name);
            }
            database.unlock().await?;
        }
        DbCommand::Migrate { db, to } => {
            let (config, mut database, _lock) = connect_locked(&db).await?;
            let done = database
                .migrate(&config.migrations_dir_path, to.as_deref())
                .await?;
            database.unlock().await?;
            for id in &done {
                println!("applied {id}");
            }
            if done.is_empty() {
                println!("nothing to apply");
            }
        }
        DbCommand::Verify { db } => {
            let (config, mut database, _lock) = connect_locked(&db).await?;
            let differences = database.verify(&config.migrations_dir_path).await?;
            database.unlock().await?;
            if differences.is_empty() {
                println!("ok: the database matches rustable's schema");
            } else {
                for d in &differences {
                    println!("difference: {d}");
                }
                return Err(RustableError::CheckFailed {
                    problems: differences.len(),
                });
            }
        }
        DbCommand::Undo { db, to } => {
            let (config, mut database, _lock) = connect_locked(&db).await?;
            let done = database.undo_to(&config.migrations_dir_path, &to).await?;
            database.unlock().await?;
            for id in &done {
                println!("undid {id}");
            }
            if done.is_empty() {
                println!("nothing to undo: {to} is the latest applied migration");
            }
        }
    }
    Ok(())
}

fn command_completions(shell: clap_complete::Shell) -> Result<(), RustableError> {
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    clap_complete::generate(shell, &mut cmd, name, &mut std::io::stdout());
    Ok(())
}

//

fn dump_error(error: RustableError) {
    eprintln!("ERROR - {error}");
    std::process::exit(1);
}

#[tokio::main]
async fn main() {
    let cli: Cli = Cli::parse();

    let res: Result<(), RustableError> = match cli.command {
        Command::Config { command } => match command {
            ConfigCommand::Show { config } => command_config_show(config.config_path).await,
        },
        Command::Analyze { config } => command_analyze(config.config_path).await,
        Command::Generate { config, no_undo } => {
            command_generate(config.config_path, no_undo).await
        }
        Command::Check { config } => command_check(config.config_path).await,
        Command::Diff { config } => command_diff(config.config_path).await,
        Command::Plan {
            config,
            soft_drops,
            write,
            name,
        } => command_plan(config.config_path, soft_drops, write, name).await,
        Command::Db { command } => command_db(command).await,
        Command::Completions { shell } => command_completions(shell),
    };

    match res {
        Ok(()) => {}
        Err(error) => dump_error(error),
    }
}

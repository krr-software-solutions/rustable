use std::path::PathBuf;

use crate::error::RustableError;
use clap::{CommandFactory, Parser, Subcommand};

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

#[derive(Debug, Subcommand)]
enum Command {
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Print a shell completion script
    Completions { shell: clap_complete::Shell },
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    Show {
        #[arg(long, env = "RUSTABLE_CONFIG_PATH")]
        config_path: PathBuf,
    },
}

//

async fn command_config_show(config_path: PathBuf) -> Result<(), RustableError> {
    let config: crate::config::InitialConfig =
        crate::config::InitialConfig::load(&config_path).await?;
    println!("Parsed Config:\n{:#?}", config);
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
    eprintln!("ERROR - {:?}", error);
    std::process::exit(1);
}

#[tokio::main]
async fn main() {
    let cli: Cli = Cli::parse();

    let res: Result<(), RustableError> = match cli.command {
        Command::Config { command } => match command {
            ConfigCommand::Show { config_path } => command_config_show(config_path).await,
        },
        Command::Completions { shell } => command_completions(shell),
    };

    match res {
        Ok(()) => {}
        Err(error) => dump_error(error),
    }
}

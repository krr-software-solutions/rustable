use clap::Parser;

/// SQL schema migrations, in Rust.
#[derive(Debug, Parser)]
#[command(name = "rustable", version)]
struct Cli {}

fn main() {
    let _cli = Cli::parse();
}

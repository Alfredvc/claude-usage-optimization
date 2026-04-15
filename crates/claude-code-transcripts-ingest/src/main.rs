mod cli;
mod parse;
mod pricing;
mod run;
mod schema;
mod serve;

use clap::Parser;

use crate::cli::{Cli, Command};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Ingest(args) => crate::run::run(args),
        Command::Serve(args) => crate::serve::run(args).await,
    }
}

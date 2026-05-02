use clap::Parser;

use claude_code_transcripts_ingest::cli::{Cli, Command};
use claude_code_transcripts_ingest::{info, run, serve};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Ingest(args) => run::run(args),
        Command::Serve(args) => serve::run(args).await,
        Command::Info(args) => info::run(args),
    }
}

use clap::Parser;

use claude_code_transcripts_ingest::cli::{Cli, Command};
use claude_code_transcripts_ingest::{info, run, serve, update, version_check};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    version_check::maybe_spawn_check(&cli.command);
    version_check::maybe_print_banner(&cli.command);
    match cli.command {
        Command::Ingest(args) => run::run(args),
        Command::Serve(args) => serve::run(args).await,
        Command::Info(args) => info::run(args),
        Command::Update(args) => update::run(args),
    }
}

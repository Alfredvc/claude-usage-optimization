use clap::Parser;

use transcript_types::ingest::cli::Cli;
use transcript_types::ingest::run::run;

fn main() {
    let cli = Cli::parse();
    run(cli);
}

mod cli;
mod parse;
mod pricing;
mod run;
mod schema;

use clap::Parser;

use crate::cli::Cli;
use crate::run::run;

fn main() {
    let cli = Cli::parse();
    run(cli);
}

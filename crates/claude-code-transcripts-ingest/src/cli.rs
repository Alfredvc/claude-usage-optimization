use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(
    name = "ingest",
    version,
    about = "Ingest Claude Code JSONL transcripts into a DuckDB database"
)]
pub struct Cli {
    /// Input directory to scan recursively for .jsonl files.
    #[arg(short = 'i', long = "input-dir", default_value = ".")]
    pub input_dir: PathBuf,

    /// Worker thread count. 0 = number of logical CPUs.
    #[arg(short = 'j', long = "jobs", default_value_t = 0)]
    pub jobs: usize,

    /// Output DuckDB filename. Overwritten on every run.
    #[arg(short = 'o', long = "output", default_value = "transcripts.duckdb")]
    pub output: PathBuf,

    /// TOML file overriding/extending the seeded model_pricing table.
    #[arg(long = "pricing")]
    pub pricing: Option<PathBuf>,

    /// Disable per-second progress reporting on stderr.
    #[arg(long = "no-progress")]
    pub no_progress: bool,
}

use std::path::PathBuf;
use std::env;

use transcript_types::check_transcript;

fn main() {
    let path: PathBuf = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            eprintln!("Usage: transcript_types <path/to/transcript.jsonl>");
            std::process::exit(1);
        });

    let result = check_transcript(&path);
    result.print_report();

    let exit_code = if result.has_errors() {
        if result.io_error.is_none() {
            println!();
        }
        1
    } else {
        println!("\n✓ all lines round-trip cleanly");
        0
    };
    std::process::exit(exit_code);
}

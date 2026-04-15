use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use rayon::prelude::*;
use walkdir::WalkDir;

use transcript_types::{TranscriptResult, check_transcript};

fn main() {
    let projects_dir = {
        let home = std::env::var("HOME").unwrap_or_else(|_| {
            eprintln!("$HOME not set");
            std::process::exit(1);
        });
        PathBuf::from(home).join(".claude").join("projects")
    };

    if !projects_dir.exists() {
        eprintln!("Not found: {}", projects_dir.display());
        std::process::exit(1);
    }

    let mut files: Vec<PathBuf> = WalkDir::new(&projects_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .filter(|p| {
            p.extension().and_then(|s| s.to_str()) == Some("jsonl")
                && p.file_name().and_then(|s| s.to_str()) != Some("permissions_log.jsonl")
        })
        .collect();

    files.sort();
    let total_files = files.len();
    eprintln!(
        "Checking {total_files} transcript files under {} on {} threads",
        projects_dir.display(),
        rayon::current_num_threads()
    );

    let checked = AtomicUsize::new(0);
    let skipped_empty = AtomicUsize::new(0);

    let failure: Option<(usize, TranscriptResult)> = files
        .par_iter()
        .enumerate()
        .find_map_any(|(idx, path)| {
            let done = checked.fetch_add(1, Ordering::Relaxed) + 1;
            if done % 500 == 0 {
                eprintln!("  {done}/{total_files}…");
            }

            let result = check_transcript(path);

            if result.total == 0 && result.io_error.is_none() {
                skipped_empty.fetch_add(1, Ordering::Relaxed);
                return None;
            }

            if result.has_errors() {
                return Some((idx + 1, result));
            }

            None
        });

    if let Some((file_no, result)) = failure {
        eprintln!("\nFailed at file {file_no}/{total_files}");
        println!();
        result.print_report();
        println!();
        std::process::exit(1);
    }

    let checked = checked.load(Ordering::Relaxed);
    let skipped_empty = skipped_empty.load(Ordering::Relaxed);
    println!(
        "✓ {checked} files checked ({skipped_empty} empty/skipped) — all round-trip cleanly"
    );
}

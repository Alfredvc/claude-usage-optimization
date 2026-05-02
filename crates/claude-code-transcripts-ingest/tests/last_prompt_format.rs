//! End-to-end test: ingest a fixture JSONL, assert the four behavior-matrix
//! rows land in `last_prompt_entries` and the all-NULL counter increments.

use std::fs;
use std::path::PathBuf;

use claude_code_transcripts_ingest::cli::IngestArgs;
use claude_code_transcripts_ingest::run::run_ingest;
use duckdb::Connection;
use tempfile::TempDir;

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/last_prompt_formats.jsonl")
}

#[test]
fn last_prompt_formats_round_trip() {
    let tmp = TempDir::new().expect("tempdir");
    let in_dir = tmp.path().join("in");
    let db_path = tmp.path().join("out.duckdb");
    fs::create_dir(&in_dir).unwrap();
    fs::copy(fixture_path(), in_dir.join("session.jsonl")).unwrap();

    let args = IngestArgs {
        input_dir: in_dir,
        jobs: 1,
        output: db_path.clone(),
        pricing: None,
        no_progress: true,
    };

    let summary = run_ingest(args).expect("ingest succeeds");

    // Acceptance criterion 4: observability counter incremented once.
    assert_eq!(
        summary
            .unknown_variants
            .get("last-prompt:no-fields")
            .copied(),
        Some(1),
        "all-NULL row must be counted exactly once: {:?}",
        summary.unknown_variants,
    );

    let conn = Connection::open(&db_path).expect("open db");

    // Acceptance criterion 3: total row count.
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM last_prompt_entries", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 4);

    // Read all rows back, ordered by entry_id (parser writes in file order).
    let rows: Vec<(Option<String>, Option<String>)> = conn
        .prepare("SELECT last_prompt, leaf_uuid FROM last_prompt_entries ORDER BY entry_id")
        .unwrap()
        .query_map([], |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, Option<String>>(1)?,
            ))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    assert_eq!(
        rows,
        vec![
            (Some("hello world".to_string()), None),
            (None, Some("u1".to_string())),
            (Some("inline".to_string()), Some("u2".to_string())),
            (None, None),
        ],
    );
}

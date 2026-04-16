use duckdb::Connection;

use crate::cli::InfoArgs;

pub fn run(args: InfoArgs) {
    let path = &args.db;

    println!("Path:    {}", path.display());

    if !path.exists() {
        println!("Status:  not found (run `cct ingest` to create)");
        return;
    }

    let size_mb = path
        .metadata()
        .map(|m| m.len() as f64 / 1_048_576.0)
        .unwrap_or(0.0);
    println!("Status:  exists ({size_mb:.1} MB)");

    let conn = match Connection::open(path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: open {}: {e}", path.display());
            std::process::exit(1);
        }
    };

    // entry count + last timestamp
    let row: Option<(i64, Option<String>)> = conn
        .query_row(
            "SELECT COUNT(*), MAX(CAST(timestamp AS VARCHAR)) FROM entries",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();

    if let Some((count, last_ts)) = row {
        println!("Entries: {}", format_count(count));
        if let Some(ts) = last_ts {
            println!("Last entry: {ts}");
        }
    }

    // session count
    let sessions: Option<i64> = conn
        .query_row(
            "SELECT COUNT(*) FROM transcripts WHERE NOT is_subagent",
            [],
            |r| r.get(0),
        )
        .ok();
    if let Some(n) = sessions {
        println!("Sessions: {}", format_count(n));
    }

    // last ingested_at
    let ingested: Option<String> = conn
        .query_row(
            "SELECT MAX(CAST(ingested_at AS VARCHAR)) FROM transcripts",
            [],
            |r| r.get(0),
        )
        .ok()
        .flatten();
    if let Some(ts) = ingested {
        println!("Last ingest: {ts}");
    }
}

fn format_count(n: i64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out.chars().rev().collect()
}

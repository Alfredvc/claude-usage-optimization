//! Typed parser for Claude Code transcript JSONL files.
//!
//! Claude Code writes one JSON object per line under `~/.claude/projects/<slug>/<session>.jsonl`.
//! This crate exposes strongly-typed [`types::Entry`] variants covering every line the
//! current client produces, plus a [`check_transcript`] round-trip validator useful for
//! pinning the schema against future Claude Code releases.

pub mod types;

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::Value;
use types::Entry;

// ---------------------------------------------------------------------------
// Public result type
// ---------------------------------------------------------------------------

pub struct TranscriptResult {
    pub path: PathBuf,
    pub total: usize,
    pub ok: usize,
    pub parse_errors: Vec<(usize, String, String)>, // (line, type, err)
    pub roundtrip_errors: Vec<(usize, String, Vec<Diff>)>,
    pub io_error: Option<String>,
}

impl TranscriptResult {
    pub fn has_errors(&self) -> bool {
        self.io_error.is_some()
            || !self.parse_errors.is_empty()
            || !self.roundtrip_errors.is_empty()
    }

    pub fn print_report(&self) {
        println!("Transcript: {}", self.path.display());
        if let Some(e) = &self.io_error {
            println!("  IO error: {e}");
            return;
        }
        println!("  Lines:     {}", self.total);
        println!("  OK:        {}", self.ok);
        println!("  Parse err: {}", self.parse_errors.len());
        println!("  RT diff:   {}", self.roundtrip_errors.len());

        if !self.parse_errors.is_empty() {
            println!("\n  ── Parse errors ──────────────────────────────────────────");
            for (line, ty, err) in &self.parse_errors {
                println!("    line {line:>4}  type={ty:30}  {err}");
            }
        }

        if !self.roundtrip_errors.is_empty() {
            println!("\n  ── Roundtrip diffs ───────────────────────────────────────");
            for (line, ty, diffs) in &self.roundtrip_errors {
                println!("    line {line:>4}  type={ty}");
                for d in diffs.iter().take(10) {
                    println!("      {d}");
                }
                if diffs.len() > 10 {
                    println!("      … ({} more)", diffs.len() - 10);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Core check logic
// ---------------------------------------------------------------------------

pub fn check_transcript(path: &Path) -> TranscriptResult {
    let mut result = TranscriptResult {
        path: path.to_owned(),
        total: 0,
        ok: 0,
        parse_errors: Vec::new(),
        roundtrip_errors: Vec::new(),
        io_error: None,
    };

    let file = match fs::File::open(path) {
        Ok(f) => f,
        Err(e) => {
            result.io_error = Some(e.to_string());
            return result;
        }
    };

    for (idx, line) in BufReader::new(file).lines().enumerate() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                result.io_error = Some(format!("IO error at line {}: {e}", idx + 1));
                return result;
            }
        };
        // Strip null bytes (can appear in corrupt JSONL lines) then whitespace.
        let line: String = line.chars().filter(|c| *c != '\0').collect();
        let line = line.trim().to_owned();
        if line.is_empty() {
            continue;
        }
        result.total += 1;

        let raw: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                result
                    .parse_errors
                    .push((idx + 1, "(not json)".into(), e.to_string()));
                continue;
            }
        };

        let entry_type = raw
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("(no type)")
            .to_owned();

        let entry: Entry = match serde_json::from_value(raw.clone()) {
            Ok(e) => e,
            Err(e) => {
                result
                    .parse_errors
                    .push((idx + 1, entry_type, e.to_string()));
                continue;
            }
        };

        let roundtripped: Value = match serde_json::to_value(&entry) {
            Ok(v) => v,
            Err(e) => {
                result
                    .parse_errors
                    .push((idx + 1, entry_type, format!("re-serialize: {e}")));
                continue;
            }
        };

        let diffs = diff_values("", &raw, &roundtripped);
        if diffs.is_empty() {
            result.ok += 1;
        } else {
            result.roundtrip_errors.push((idx + 1, entry_type, diffs));
        }
    }

    result
}

// ---------------------------------------------------------------------------
// Recursive value diff
// ---------------------------------------------------------------------------

pub struct Diff(pub String);

impl std::fmt::Display for Diff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub fn diff_values(path: &str, a: &Value, b: &Value) -> Vec<Diff> {
    let mut out = Vec::new();
    diff_inner(path, a, b, &mut out);
    out
}

fn diff_inner(path: &str, a: &Value, b: &Value, out: &mut Vec<Diff>) {
    match (a, b) {
        (Value::Object(ma), Value::Object(mb)) => {
            for (k, va) in ma {
                let child = child_path(path, k);
                match mb.get(k) {
                    None => out.push(Diff(format!("missing in output:  {child} = {va}"))),
                    Some(vb) => diff_inner(&child, va, vb, out),
                }
            }
            for k in mb.keys() {
                if !ma.contains_key(k) {
                    let child = child_path(path, k);
                    out.push(Diff(format!("extra in output:    {child} = {}", mb[k])));
                }
            }
        }
        (Value::Array(aa), Value::Array(ab)) => {
            if aa.len() != ab.len() {
                out.push(Diff(format!(
                    "array length mismatch at {path}: {} vs {}",
                    aa.len(),
                    ab.len()
                )));
                return;
            }
            for (i, (va, vb)) in aa.iter().zip(ab.iter()).enumerate() {
                diff_inner(&format!("{path}[{i}]"), va, vb, out);
            }
        }
        (Value::Number(na), Value::Number(nb)) => {
            let fa = na.as_f64().unwrap_or(f64::NAN);
            let fb = nb.as_f64().unwrap_or(f64::NAN);
            if (fa - fb).abs() > f64::EPSILON {
                out.push(Diff(format!("value mismatch at {path}: {a} vs {b}")));
            }
        }
        _ => {
            if a != b {
                let label = if path.is_empty() { "(root)" } else { path };
                out.push(Diff(format!("value mismatch at {label}: {a} vs {b}")));
            }
        }
    }
}

fn child_path(parent: &str, key: &str) -> String {
    if parent.is_empty() {
        key.to_owned()
    } else {
        format!("{parent}.{key}")
    }
}

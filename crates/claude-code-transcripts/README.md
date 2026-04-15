# claude-code-transcripts

[![crates.io](https://img.shields.io/crates/v/claude-code-transcripts.svg)](https://crates.io/crates/claude-code-transcripts)
[![docs.rs](https://img.shields.io/docsrs/claude-code-transcripts)](https://docs.rs/claude-code-transcripts)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)

Typed parser for Claude Code transcript JSONL files.

Claude Code writes one JSON object per line into `~/.claude/projects/<slug>/<session>.jsonl`.
This crate exposes strongly-typed `Entry` variants covering every line kind the current
client emits (user, assistant, system, summary, attachments, progress, tool uses, tool
results, usage blocks, cache tokens, etc.), plus a round-trip validator useful for
catching schema drift when Claude Code ships new fields.

## Install

```sh
cargo add claude-code-transcripts
```

## Usage

```rust,ignore
use claude_code_transcripts::types::Entry;

let text = std::fs::read_to_string("session.jsonl")?;
for line in text.lines().filter(|l| !l.is_empty()) {
    let entry: Entry = serde_json::from_str(line)?;
    // match on entry variants …
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Round-trip validator

Parse every line of a transcript and diff the re-serialized JSON against the original
to detect unknown fields:

```rust,ignore
let result = claude_code_transcripts::check_transcript(std::path::Path::new("session.jsonl"));
result.print_report();
```

Two examples ship in-tree:

```sh
cargo run --example check_one -- path/to/session.jsonl
cargo run --example check_all                             # scans ~/.claude/projects
```

## License

Dual-licensed under [MIT](LICENSE-MIT) OR [Apache-2.0](LICENSE-APACHE).

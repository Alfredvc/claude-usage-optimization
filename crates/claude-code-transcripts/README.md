# claude-code-transcripts

Typed parser for Claude Code transcript JSONL files.

Claude Code writes one JSON object per line into `~/.claude/projects/<slug>/<session>.jsonl`.
This crate exposes strongly-typed `Entry` variants covering every line kind the current
client emits (user, assistant, system, summary, attachments, progress, tool uses, tool
results, usage blocks, cache tokens, etc.), plus a round-trip validator useful for
catching schema drift when Claude Code ships new fields.

## Usage

```rust
use claude_code_transcripts::types::Entry;

let line = std::fs::read_to_string("session.jsonl")?;
for l in line.lines().filter(|l| !l.is_empty()) {
    let entry: Entry = serde_json::from_str(l)?;
    // match on entry variants …
}
```

## Round-trip validator

Parse every line of a transcript and diff the re-serialized JSON against the original
to detect unknown fields:

```rust
let result = claude_code_transcripts::check_transcript(std::path::Path::new("session.jsonl"));
result.print_report();
```

Two examples ship in-tree for local development:

```sh
cargo run --example check_one -- path/to/session.jsonl
cargo run --example check_all                             # scans ~/.claude/projects
```

## License

MIT OR Apache-2.0

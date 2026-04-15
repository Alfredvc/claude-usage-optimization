<h1 align="center">cct</h1>

<p align="center">
  <img src="docs/readme-image.jpg" alt="cct banner" width="800" />
</p>

<p align="center">
  <a href="https://github.com/alfredvc/claude-usage-optimization/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/alfredvc/claude-usage-optimization/ci.yml?branch=main&label=CI" alt="CI" /></a>
  <a href="https://github.com/alfredvc/claude-usage-optimization/releases"><img src="https://img.shields.io/github/v/release/alfredvc/claude-usage-optimization" alt="Release" /></a>
  <a href="https://crates.io/crates/claude-code-transcripts-ingest"><img src="https://img.shields.io/crates/v/claude-code-transcripts-ingest.svg" alt="crates.io" /></a>
  <a href="LICENSE-MIT"><img src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue" alt="License: MIT OR Apache-2.0" /></a>
</p>

Ingest every Claude Code JSONL transcript under `~/.claude/projects` into a single DuckDB file, then query it with SQL or explore it in the embedded React viewer.

## Install

```bash
cargo install claude-code-transcripts-ingest
```

Installs the `cct` binary. DuckDB is built from bundled C++ sources — first install takes a minute.

## Agent Integration

### AGENTS.md

See [AGENTS.md](AGENTS.md).

### Agent Skills

Install for Claude Code, Cursor, Gemini CLI, etc:

```bash
npx skills add alfredvc/claude-usage-optimization
```

Available skills:

- **claude-usage-db** — gives the agent everything it needs to query the transcripts DB safely: schema layout, sidechain/subagent model, JSON column shapes, billing-safety rules, and a library of ready-to-run SQL recipes for cost, token, tool-use, and session analysis.
- **optimize-usage** — diagnostic methodology for turning the DB into actionable cost recommendations. Guides the agent past shallow category rollups toward root causes (artifact propagation, context bloat, workflow cycles) with phase gates that prevent premature victory declaration. Built on top of `claude-usage-db`.

## Quick Start

```bash
cct ingest                                          # scans ~/.claude/projects → ./transcripts.duckdb
cct serve                                           # viewer at http://localhost:8766
duckdb transcripts.duckdb "SELECT ROUND(SUM(cost_usd),2) FROM assistant_entries_deduped WHERE message_id IS NOT NULL;"
```

## Commands

### `cct ingest`

Scan JSONL transcripts and write a DuckDB database.

```
cct ingest [-i <dir>] [-o <file>] [-j <jobs>] [--pricing <toml>] [--no-progress]
```

| Flag | Default | Meaning |
|---|---|---|
| `-i, --input-dir` | `~/.claude/projects` | Directory scanned recursively for `.jsonl` |
| `-o, --output` | `./transcripts.duckdb` | Output DuckDB file (overwritten each run) |
| `-j, --jobs` | `0` (logical CPUs) | Parallel worker threads |
| `--pricing` | — | TOML overriding the seeded `model_pricing` table |
| `--no-progress` | — | Silence per-second progress on stderr |

### `cct serve`

Serve the embedded viewer backed by a DuckDB file.

```
cct serve [--db <file>] [--port <n>]
```

| Flag | Default | Meaning |
|---|---|---|
| `--db` | `./transcripts.duckdb` | DB file to serve |
| `--port` | `8766` | Listen port |

## Workspace

```
crates/claude-code-transcripts/          # typed parser library (no DuckDB)
crates/claude-code-transcripts-ingest/   # `cct` binary (ingest + serve)
web/index.html                           # embedded React viewer
skills/                                  # agent skills (see above)
docs/cost-attribution.md                 # why dedup by (file_path, message_id) is safe
```

The parser crate ([`claude-code-transcripts`](https://crates.io/crates/claude-code-transcripts)) is independently usable — strongly-typed `Entry` variants and a round-trip validator for catching schema drift.

## Development

- `cargo build` — build workspace
- `cargo test` — unit + integration tests
- `cargo clippy --all-targets --all-features`
- `cargo fmt`
- Pre-commit hook (`.git/hooks/pre-commit`) runs `fmt` + `clippy`

## License

Dual-licensed under [MIT](LICENSE-MIT) OR [Apache-2.0](LICENSE-APACHE).

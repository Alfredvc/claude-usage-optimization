# claude-code-transcripts-ingest

[![crates.io](https://img.shields.io/crates/v/claude-code-transcripts-ingest.svg)](https://crates.io/crates/claude-code-transcripts-ingest)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)

CLI that ingests every Claude Code transcript under `~/.claude/projects` into a
DuckDB database, with a normalised schema suited for usage / cost analysis across
sessions, subagents, tool calls, and cache tokens. Ships with an embedded viewer
served over HTTP.

Installs the `cct` binary.

## Install

```sh
cargo install claude-code-transcripts-ingest
```

The `duckdb` dependency is bundled (built from C++ sources), so the install is
self-contained but takes a minute or two the first time.

## Quick start

```sh
cct ingest                # scans ~/.claude/projects → ./transcripts.duckdb
cct serve                 # viewer at http://localhost:8766
```

## Commands

### `cct ingest`

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

```
cct serve [--db <file>] [--port <n>]
```

| Flag | Default | Meaning |
|---|---|---|
| `--db` | `./transcripts.duckdb` | DB file to serve |
| `--port` | `8766` | Listen port |

#### Transcripts

Browse by project → session → turn-by-turn timeline. Every assistant turn shows its exact cost: input, output, cache-read, and cache-creation tokens with the resulting dollar amount. Subagent calls expand inline so you can trace the full cost of any delegated task back to the turn that triggered it.

![cct serve transcripts](https://raw.githubusercontent.com/alfredvc/claude-usage-optimization/main/docs/assets/transcripts.png)

#### Dashboard

Shows total spend, daily cost by model (Opus / Sonnet / Haiku), cache hit rate, agent model inheritance (subagents that silently fell back to Opus), top sessions by cost, file hotspots (files re-read across the most sessions), and error cost premium.

![cct serve dashboard](https://raw.githubusercontent.com/alfredvc/claude-usage-optimization/main/docs/assets/dashboard.png)

Run `cct --help` / `cct <subcommand> --help` for the authoritative flag list.

## Library

The transcript parser lives in [`claude-code-transcripts`](https://crates.io/crates/claude-code-transcripts)
and can be used standalone without DuckDB.

## License

Dual-licensed under [MIT](LICENSE-MIT) OR [Apache-2.0](LICENSE-APACHE).

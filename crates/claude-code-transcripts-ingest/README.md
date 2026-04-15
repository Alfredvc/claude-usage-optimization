# claude-code-transcripts-ingest

CLI that ingests every Claude Code transcript under `~/.claude/projects` into a
DuckDB database, with a normalised schema suited for usage / cost analysis
across sessions, subagents, tool calls, and cache tokens.

## Install

```sh
cargo install claude-code-transcripts-ingest
```

The `duckdb` dependency is bundled (built from C++ sources) so the install is
self-contained but takes a minute or two the first time.

## Usage

```sh
claude-code-transcripts-ingest \
    --projects-dir ~/.claude/projects \
    --db ./transcripts.duckdb \
    --pricing ./pricing.toml
```

Run `claude-code-transcripts-ingest --help` for all flags.

## Library

The transcript parser lives in [`claude-code-transcripts`](https://crates.io/crates/claude-code-transcripts)
and can be used standalone without DuckDB.

## License

MIT OR Apache-2.0

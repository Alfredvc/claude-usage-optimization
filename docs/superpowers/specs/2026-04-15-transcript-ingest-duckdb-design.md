# Transcript Ingestion to DuckDB — Design Spec

**Date:** 2026-04-15
**Status:** Approved (pending user spec review)
**Owner:** Alfredo V. Clemente

---

## Goal

Build a Rust binary `ingest` that walks a directory of Claude Code JSONL transcripts and writes their full contents into a single DuckDB database. The database is the analytical substrate for the `claude-usage-visualizer` project: any question about Claude Code usage — cost, tool use, sessions, errors, file edits — must be answerable via plain SQL against this DB.

Three constraints drive every design choice:

1. **Schema fully known statically.** No surprise fields. Every variant in `src/types.rs` maps to a known table or known JSON column.
2. **Fully understood by LLMs.** A coding agent reading `DESCRIBE` output must be able to write correct queries without prior knowledge of the project.
3. **Answers all questions.** No information loss vs. the source JSONL.

---

## Format Choice: DuckDB

Picked over SQLite, Parquet, Arrow:

- **Static schema:** DuckDB STRUCT/LIST types map directly to Rust enums. No flatten-or-stringify forced by the format.
- **LLM fluency:** SQL is universal. DuckDB CLI mirrors `sqlite3` UX. Schema introspection via `DESCRIBE`, `.schema`, `.tables`.
- **Polymorphic data:** native nested types where shape is known; JSON columns where shape varies (per-tool input, ad-hoc API fields).
- **Speed:** columnar storage, 10–100× SQLite for analytics aggregations.
- **Escape hatch:** `COPY ... TO 'foo.parquet'` exports any subset.
- **CLI:** `brew install duckdb`; `duckdb transcripts.duckdb "SELECT ..."` works one-shot or interactive.

---

## CLI

New bin `src/bin/ingest.rs`. Cargo entry `[[bin]] name = "ingest"`.

```
ingest [OPTIONS]

Options:
  -i, --input-dir <DIR>     Input directory to scan recursively for .jsonl files.
                            [default: current working directory]
  -j, --jobs <N>            Worker thread count.
                            [default: number of logical CPUs]
  -o, --output <FILE>       Output DuckDB filename.
                            [default: transcripts.duckdb]
      --pricing <FILE>      TOML file overriding/extending the seeded model_pricing table.
                            Loaded after seed; rows replace seed rows on `model` collision.
      --skip-existing       Skip files already ingested with the same mtime.
                            Otherwise re-ingest (delete prior rows for that file_path).
      --no-progress         Disable per-second progress reporting on stderr.
  -h, --help
  -V, --version
```

Skips `permissions_log.jsonl` (matches existing `check_all.rs`).

Default output filename intentionally uses `.duckdb` extension (DuckDB convention) rather than the `.ts` typo from the original request.

---

## New Dependencies

Add to `Cargo.toml`:

- `clap` (with `derive` feature) — CLI parsing.
- `duckdb` — DuckDB Rust binding (FFI to libduckdb). Use `bundled` feature to avoid system-install requirement.
- `chrono` (with `serde` feature) — timestamp parsing for `TIMESTAMP` columns.
- `crossbeam-channel` — bounded MPSC for worker → writer pipeline.
- `toml` — `--pricing` override file parsing.

Existing deps kept: `serde`, `serde_json`, `walkdir`, `rayon`.

---

## Schema

### Tables (~30 total)

#### Core (3)

**`transcripts`** — file-level metadata.

| col | type | notes |
|---|---|---|
| `file_path` | TEXT PRIMARY KEY | absolute or canonicalised path |
| `session_id` | TEXT | from filename / first envelope |
| `is_subagent` | BOOLEAN | true if file lives under `subagents/` dir |
| `agent_id` | TEXT NULL | from filename `agent-{id}.jsonl` |
| `parent_session_id` | TEXT NULL | parent session for subagent files (derived from filesystem path) |
| `entry_count` | INTEGER | parsed lines (after blank skip) |
| `first_timestamp` | TIMESTAMP NULL | min over message-bearing entries |
| `last_timestamp` | TIMESTAMP NULL | max over message-bearing entries |
| `mtime` | TIMESTAMP | for `--skip-existing` |
| `ingested_at` | TIMESTAMP | when this row was written |

**`entries`** — one row per JSONL line. Surrogate key + full envelope.

| col | type | notes |
|---|---|---|
| `entry_id` | BIGINT PRIMARY KEY | surrogate (sequence) |
| `file_path` | TEXT REFERENCES transcripts | |
| `line_no` | INTEGER | 1-based |
| `type` | TEXT | top-level discriminator (user, assistant, system, …) |
| `subtype` | TEXT NULL | e.g. system subtype |
| `uuid` | TEXT NULL | message envelope uuid (NULL for metadata-only variants) |
| `parent_uuid` | TEXT NULL | |
| `logical_parent_uuid` | TEXT NULL | |
| `is_sidechain` | BOOLEAN NULL | |
| `session_id` | TEXT NULL | |
| `timestamp` | TIMESTAMP NULL | |
| `user_type` | TEXT NULL | |
| `entrypoint` | TEXT NULL | |
| `cwd` | TEXT NULL | |
| `version` | TEXT NULL | |
| `git_branch` | TEXT NULL | |
| `slug` | TEXT NULL | |
| `agent_id` | TEXT NULL | |
| `team_name` | TEXT NULL | |
| `agent_name` | TEXT NULL | |
| `agent_color` | TEXT NULL | |
| `prompt_id` | TEXT NULL | |
| `is_meta` | BOOLEAN NULL | |
| `forked_from_uuid` | TEXT NULL | |
| `forked_from_session_id` | TEXT NULL | |

> **Note:** the original JSONL line is intentionally not stored. `file_path` + `line_no` is the canonical pointer back to the source file; reconstruct on demand with `awk "NR==<line_no>" <file_path>`. Storing the raw line roughly doubled DB size on real data (~1.9 GB on a 3.9 GB DB).

**`model_pricing`** — pricing reference, USD per 1M tokens.

| col | type | notes |
|---|---|---|
| `model` | TEXT PRIMARY KEY | exact match against `assistant_entries.model` |
| `input_per_mtok` | DOUBLE | |
| `output_per_mtok` | DOUBLE | |
| `cache_creation_5m_per_mtok` | DOUBLE | |
| `cache_creation_1h_per_mtok` | DOUBLE | |
| `cache_read_per_mtok` | DOUBLE | |
| `effective_date` | DATE | |

Seeded at ingest start; `--pricing <FILE>` rows override on collision.

#### Rich variant tables (5 — 1:1 with `entries` via `entry_id`)

- **`user_entries`** — `entry_id PK`, `message_role`, `message_content_text` (when content is plain string), `message_has_blocks` (BOOLEAN), `tool_use_result JSON NULL`, `source_tool_assistant_uuid`, `source_tool_use_id`, `permission_mode`, `origin JSON`, `is_compact_summary`, `is_visible_in_transcript_only`, `image_paste_ids INTEGER[]`, `plan_content`.

- **`assistant_entries`** — `entry_id PK`, `message_id`, `role`, `model`, `container JSON`, `stop_reason`, `stop_sequence`, `stop_details JSON`, `context_management JSON`, `request_id`, `is_api_error_message`, `error`, `tool_use_count INTEGER`, `cost_usd DOUBLE`, plus flat usage columns: `input_tokens`, `output_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens`, `cache_creation_5m`, `cache_creation_1h`, `web_search_requests`, `web_fetch_requests`, `service_tier JSON`, `inference_geo JSON`, `iterations JSON`, `speed JSON`. Generated column `cost_per_tool_use DOUBLE GENERATED AS (cost_usd / NULLIF(tool_use_count, 0)) VIRTUAL`.

- **`system_entries`** — `entry_id PK`, `subtype`, `content`, `level`, `is_meta`, `cause JSON`, `error JSON`, `retry_in_ms`, `retry_attempt`, `max_retries`, `hook_count`, `hook_errors JSON`, `prevented_continuation`, `stop_reason`, `has_output`, `tool_use_id`, `duration_ms`, `message_count`, `url`, `upgrade_nudge`, `compact_metadata STRUCT(trigger TEXT, pre_tokens BIGINT, post_tokens BIGINT, duration_ms BIGINT, preserved_segment STRUCT(head_uuid TEXT, anchor_uuid TEXT, tail_uuid TEXT), pre_compact_discovered_tools TEXT[])`.

- **`attachment_entries`** — `entry_id PK`, `attachment_type` (discriminator string), then a wide set of nullable typed columns covering all attachment subtypes: `hook_name`, `tool_use_id`, `hook_event`, `hook_content`, `hook_stdout`, `hook_stderr`, `hook_exit_code`, `hook_command`, `hook_duration_ms`, `decision`, `filename`, `file_content_text`, `file_content_metadata JSON`, `display_path`, `directory_path`, `directory_content`, `command_allowed_tools TEXT[]`, `plan_reminder_type`, `plan_is_sub_agent`, `plan_file_path`, `plan_exists`, `skill_listing_content`, `skill_listing_is_initial`, `skill_listing_count`, `skill_dir`, `skill_names TEXT[]`, `invoked_skills JSON`, `task_reminder_content JSON`, `task_reminder_item_count`, `diagnostics_files JSON`, `diagnostics_is_new`, `date_change_new_date`, `deferred_added_names TEXT[]`, `deferred_added_lines TEXT[]`, `deferred_removed_names TEXT[]`, `mcp_added_names TEXT[]`, `mcp_added_blocks TEXT[]`, `mcp_removed_names TEXT[]`, `ultrathink_level`, `queued_command_prompt`, `queued_command_mode`. (Fields with no clean static shape go to JSON.)

- **`progress_entries`** — `entry_id PK`, `parent_tool_use_id`, `tool_use_id`, and flat `ProgressData` cols: `data_type`, `hook_event`, `hook_name`, `command`, `agent_id`, `prompt`, `message JSON`, plus any further `ProgressData` fields present in `types.rs` at implementation time. Polymorphic `message` payload stays JSON.

#### Child tables (8)

- **`user_content_blocks`** — `entry_id`, `position`, `block_type` (text|tool_result|image|document), `text`, `tool_use_id`, `tool_result_content JSON`, `is_error`, `image_source JSON`, `document_source JSON`, `document_title`. Composite key `(entry_id, position)`.

- **`assistant_content_blocks`** — `entry_id`, `position`, `block_type` (text|thinking|redacted_thinking|tool_use), `text`, `thinking`, `signature`, `redacted_data`, `tool_use_id`, `tool_name`, `tool_input JSON`, `caller_type`. Composite key `(entry_id, position)`.

- **`tool_uses`** — VIEW over `assistant_content_blocks` filtered on `block_type = 'tool_use'`, with extracted columns:

  ```sql
  CREATE VIEW tool_uses AS
  SELECT
    entry_id,
    position,
    tool_use_id,
    tool_name AS name,
    tool_input AS input,
    caller_type,
    json_extract_string(tool_input, '$.file_path')      AS input_file_path,
    json_extract_string(tool_input, '$.notebook_path')  AS input_notebook_path,
    json_extract_string(tool_input, '$.path')           AS input_path,
    json_extract_string(tool_input, '$.command')        AS input_command,
    COALESCE(
      json_extract_string(tool_input, '$.file_path'),
      json_extract_string(tool_input, '$.notebook_path'),
      json_extract_string(tool_input, '$.path')
    ) AS effective_path,
    regexp_extract(
      COALESCE(
        json_extract_string(tool_input, '$.file_path'),
        json_extract_string(tool_input, '$.notebook_path'),
        json_extract_string(tool_input, '$.path')),
      '\.([^.]+)$', 1
    ) AS file_ext
  FROM assistant_content_blocks
  WHERE block_type = 'tool_use';
  ```

- **`assistant_usage_iterations`** — `entry_id`, `position`, `iter_type`, `input_tokens`, `output_tokens`, `cache_read_input_tokens`, `cache_creation_input_tokens`, `cache_creation_5m`, `cache_creation_1h`. Populated only when `usage.iterations` is a typed array.

- **`system_hook_infos`** — `entry_id`, `position`, `command`, `duration_ms` (children of `system` `stop_hook_summary`).

- **`attachment_diagnostics_files`** — `entry_id`, `position`, file-level diagnostic fields (extracted from `attachment.files`).

- **`attachment_invoked_skills`** — `entry_id`, `position`, `skill_name`, `invocation_metadata JSON`.

- (Other attachment children added only if their shape benefits from a row-per-element view; otherwise kept as `LIST<STRUCT>` on the parent.)

#### Metadata-only variant tables (~20)

One per non-message-bearing variant in `Entry`. Pattern: `entry_id BIGINT PRIMARY KEY`, then variant-specific columns in their natural shapes.

Names: `permission_mode_entries`, `last_prompt_entries`, `ai_title_entries`, `custom_title_entries`, `agent_name_entries`, `agent_color_entries`, `agent_setting_entries`, `tag_entries`, `summary_entries`, `task_summary_entries`, `pr_link_entries`, `mode_entries`, `worktree_state_entries`, `content_replacement_entries`, `file_history_snapshot_entries`, `attribution_snapshot_entries`, `queue_operation_entries`, `marble_origami_commit_entries`, `marble_origami_snapshot_entries`, `speculation_accept_entries`.

Some are sparse (a few rows project-wide). Kept as their own tables for schema clarity — empty tables cost nothing.

### Polymorphic JSON columns (cannot be statically typed)

These remain as DuckDB `JSON` columns; queryable via `json_extract`, `json_extract_string`, `->`, `->>`:

- `tool_uses.input` — per-tool shape (60+ tools).
- `user_content_blocks.tool_result_content` — string or mixed-type array.
- `system_entries.cause`, `system_entries.error`, `system_entries.hook_errors`.
- `assistant_entries.container`, `stop_details`, `context_management`, `service_tier`, `inference_geo`, `iterations`, `speed`.
- `attachment_entries.*` payloads where shape varies.

---

## Cost Computation

For each `assistant` entry with a known model:

```
cost_usd = ( input_tokens                     × price.input_per_mtok
           + output_tokens                    × price.output_per_mtok
           + COALESCE(cache_creation_5m, 0)   × price.cache_creation_5m_per_mtok
           + COALESCE(cache_creation_1h, 0)   × price.cache_creation_1h_per_mtok
           + COALESCE(cache_read_input_tokens, 0) × price.cache_read_per_mtok
           ) / 1_000_000
```

If no `cache_creation_5m`/`cache_creation_1h` breakdown is present but `cache_creation_input_tokens` is, treat the whole as 5m (the conservative default; matches Anthropic SDK behavior pre-1h support).

Unknown model → `cost_usd = NULL`. Each unknown model name logged once to stderr.

`tool_use_count` stored at ingest = number of `tool_use` blocks in the assistant message. `cost_per_tool_use` = generated column `cost_usd / NULLIF(tool_use_count, 0)`.

### Pricing seed (USD per 1M tokens)

| Model | Input | Output | Cache 5m | Cache 1h | Cache read |
|---|---|---|---|---|---|
| `claude-opus-4-6` | 15.00 | 75.00 | 18.75 | 30.00 | 1.50 |
| `claude-sonnet-4-6` | 3.00 | 15.00 | 3.75 | 6.00 | 0.30 |
| `claude-haiku-4-5` | 1.00 | 5.00 | 1.25 | 2.00 | 0.10 |
| `claude-3-5-sonnet-20241022` | 3.00 | 15.00 | 3.75 | 6.00 | 0.30 |
| `claude-3-5-haiku-20241022` | 0.80 | 4.00 | 1.00 | 1.60 | 0.08 |
| `claude-3-opus-20240229` | 15.00 | 75.00 | 18.75 | 30.00 | 1.50 |

Anthropic pricing convention: cache-write-5m = 1.25× input price; cache-write-1h = 2× input price; cache-read = 0.10× input price. **Verify these numbers against the Anthropic pricing page before committing the seed.** `--pricing <FILE>` overrides at ingest.

Model strings are matched **exactly**. No automatic family/version derivation. Variants like `claude-opus-4-6[1m]` (the 1M-context flag suffix) require an explicit row in `model_pricing` (or via `--pricing`); otherwise `cost_usd = NULL`.

---

## Concurrency

**Rule:** DuckDB is single-writer per process. Parse-parallel, write-serial.

Pipeline:

1. Walk `--input-dir` recursively → collect `.jsonl` paths (skip `permissions_log.jsonl`).
2. Build a `rayon` thread pool with `--jobs` workers.
3. Each worker parses one file at a time → emits `Vec<Row>` per target table (in-memory batch).
4. Worker pushes batch to writer thread via bounded `crossbeam_channel` (capacity ≈ `jobs * 2` for backpressure).
5. Single writer thread: pulls batch, opens one transaction per file, uses DuckDB `Appender` API (~10× faster than prepared INSERTs) per table, commits.
6. Optional progress thread: prints `N/M files, X entries/s` to stderr every second unless `--no-progress`.

Per-file transactions chosen over a single mega-transaction for: bounded peak memory, partial-progress durability on crash, amortised checkpoint cost.

`--skip-existing`: writer queries `SELECT mtime FROM transcripts WHERE file_path = ?` before parsing; matches → skip. Otherwise re-ingest. Re-ingest deletion sequence (no FK cascade — DuckDB FK enforcement is limited; do it explicitly):

1. `DELETE FROM <child_table> WHERE entry_id IN (SELECT entry_id FROM entries WHERE file_path = ?)` for every child table (`user_content_blocks`, `assistant_content_blocks`, `assistant_usage_iterations`, `system_hook_infos`, `attachment_diagnostics_files`, `attachment_invoked_skills`).
2. `DELETE FROM <variant_table> WHERE entry_id IN (...)` for every per-variant table.
3. `DELETE FROM entries WHERE file_path = ?`.
4. `DELETE FROM transcripts WHERE file_path = ?`.

Wrapped in the same per-file transaction as the subsequent re-ingest.

Schema init is idempotent (`CREATE TABLE IF NOT EXISTS`). DDL runs on writer thread before any data flows.

Parse failure on one file → log file + line + error to stderr, skip file, continue. Process exit code = 1 if any file failed; 0 otherwise.

---

## Indexes

Created at end of ingest after bulk loads:

- `entries(session_id)`, `entries(timestamp)`, `entries(type)`, `entries(parent_uuid)`
- `assistant_entries(model)`, `assistant_entries(cost_usd)`
- `assistant_content_blocks(entry_id, position)`, `assistant_content_blocks(tool_name)`
- `attachment_entries(attachment_type)`
- `transcripts(session_id)`

---

## End-of-run report (stderr)

```
Files:        N processed, M skipped (empty), K failed
Entries:      total=…
              user=…  assistant=…  system=…  attachment=…  metadata=…
Unknown models (cost_usd = NULL):
  - <model>  (count=…)
Elapsed:      HH:MM:SS
```

---

## Testing

- **Unit tests:** parser fixtures — 5–10 hand-picked transcript lines covering each `Entry` variant; assert correct row tuples per table.
- **Integration test:** ingest a small fixture directory → run `duckdb fixture.duckdb -c "<query>"` and assert results.
- **Regression:** keep existing `check_all` round-trip parser. New ingest must not silently drop any line that `check_all` accepts.

---

## Out of Scope (YAGNI)

- Incremental ingest beyond `--skip-existing` (no in-place updates within a file).
- Schema migration framework — schema rebuilds from scratch on change.
- Watch / daemon mode.
- Web UI (separate concern; `index.html`/`server.py` already exist).
- Multi-database merging.
- Re-deriving cost from pricing table after ingest — query-time joins to `model_pricing` work, but `cost_usd` on `assistant_entries` is a snapshot at ingest time.

---

## Target query — verification

```sql
SELECT
  tu.file_ext,
  SUM(ae.cost_per_tool_use)        AS attributed_cost_usd,
  COUNT(*)                         AS write_count
FROM tool_uses tu
JOIN assistant_entries ae USING (entry_id)
WHERE tu.name = 'Write'
  AND tu.file_ext IS NOT NULL
GROUP BY tu.file_ext
ORDER BY attributed_cost_usd DESC;
```

Resolves with one join, one filter, one group — schema goal achieved.

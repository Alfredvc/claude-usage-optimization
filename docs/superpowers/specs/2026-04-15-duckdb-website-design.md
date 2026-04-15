# DuckDB-Backed Website Design

**Date:** 2026-04-15  
**Status:** Approved

## Goal

Port the existing Claude Usage Visualizer website from reading raw JSONL files to querying the DuckDB database (`transcripts.duckdb`). Same views and visual design. No JSONL reads at runtime.

## Scope

- New Rust/Axum HTTP server (`src/bin/serve.rs`) replacing `server.py`
- New frontend (`web/index.html`) replacing JSONL parsing with structured JSON rendering
- Existing `index.html` + `server.py` left untouched

## Architecture

```
Browser (web/index.html)
  ↕ JSON API
src/bin/serve.rs  (Axum, port 8766)
  ↕ DuckDB queries (spawn_blocking)
transcripts.duckdb
```

**DuckDB access:** `Arc<Mutex<duckdb::Connection>>` as Axum shared state. All queries run in `tokio::task::spawn_blocking`. Read-only connection — no WAL conflicts with the `ingest` binary.

## New Cargo Dependencies

```toml
axum = "0.7"
tokio = { version = "1", features = ["full"] }
```

`duckdb`, `serde`, `serde_json` already present.

## File Layout

```
src/bin/serve.rs      ← new Axum server binary
web/
  index.html          ← new DB-backed frontend
index.html            ← unchanged (existing JSONL site)
server.py             ← unchanged
Cargo.toml            ← add axum + tokio
```

## API Endpoints

All `GET`. Same route shape as `server.py`.

### `GET /`
Serve `web/index.html`.

### `GET /api/projects`
Returns projects derived from `transcripts` table, grouped by project key extracted from `file_path`.

```sql
SELECT
  regexp_extract(file_path, '.*/projects/([^/]+)/[^/]+\.jsonl$', 1) AS project_key,
  COUNT(*) AS session_count,
  MAX(e.timestamp) AS last_active
FROM transcripts t
JOIN entries e ON e.file_path = t.file_path
WHERE NOT t.is_subagent
GROUP BY project_key
ORDER BY last_active DESC
```

Response shape:
```json
[{ "key": "...", "display": "~/path/to/project", "sessionCount": 3 }]
```

`display` computed in Rust using same un-slugging algorithm as `server.py` (`display_name`).

### `GET /api/sessions?project=<key>`
Returns sessions with accurate cost from `assistant_entries_deduped`.

```sql
SELECT
  t.session_id,
  MIN(e.timestamp) AS started_at,
  MAX(e.timestamp) AS last_active,
  ROUND(SUM(d.cost_usd), 6) AS cost_usd,
  EXISTS(
    SELECT 1 FROM transcripts t2
    WHERE t2.parent_session_id = t.session_id
  ) AS has_subagents
FROM transcripts t
JOIN entries e ON e.file_path = t.file_path
LEFT JOIN assistant_entries_deduped d ON d.entry_id = e.entry_id AND d.message_id IS NOT NULL
WHERE NOT t.is_subagent
  AND regexp_extract(t.file_path, '.*/projects/([^/]+)/[^/]+\.jsonl$', 1) = ?
GROUP BY t.session_id
ORDER BY last_active DESC
```

Response shape:
```json
[{ "id": "<uuid>", "startedAt": "...", "lastActive": "...", "costUsd": 0.123, "hasSubagents": true }]
```

### `GET /api/transcript?project=<key>&session=<id>`

Returns pre-built timeline JSON. Server joins `entries`, `assistant_entries_deduped`, `assistant_content_blocks`, `user_content_blocks` to assemble the full timeline.

Assembly steps:
1. Fetch all entries for the session ordered by `entry_id` (non-sidechain, non-subagent file)
2. For user entries: collect text blocks from `user_content_blocks` (block_type='text'); skip injected system messages
3. For assistant entries: use `assistant_entries_deduped` to skip duplicates; collect content blocks from `assistant_content_blocks`; link tool_use ids to tool results via `user_content_blocks` (block_type='tool_result')
4. Extract `agent_id` from tool result text (`agentId: <uuid>` pattern) to identify subagent calls

### `GET /api/subagent?session=<id>&agent=<agent_id>`

Same timeline assembly as `/api/transcript` but for a subagent transcript (look up `transcripts` where `agent_id = ?` and `parent_session_id = ?`).

## Timeline JSON Shape

```json
{
  "entries": [
    {
      "kind": "user",
      "timestamp": "2026-04-15T10:00:00Z",
      "text": "user message text"
    },
    {
      "kind": "assistant",
      "num": 1,
      "model": "claude-sonnet-4-6",
      "cost_usd": 0.0123,
      "input_tokens": 1000,
      "output_tokens": 500,
      "cache_read_input_tokens": 200,
      "cache_creation_input_tokens": 0,
      "has_thinking": true,
      "texts": ["response text"],
      "tool_uses": [
        {
          "id": "toolu_xxx",
          "name": "Read",
          "input": { "file_path": "/some/file" },
          "result": "file contents...",
          "agent_id": null
        }
      ]
    }
  ]
}
```

`kind` is either `"user"` or `"assistant"`.

## Frontend (`web/index.html`)

**Kept from current `index.html`:**
- All CSS / design tokens (dark theme, color vars)
- `UserCard`, `ApiCard`, `DualBars`, `SubagentCard` components
- `fmtTok`, `fmtCost`, `fmtDate` formatters
- Pricing constants (used for cost bar color breakdown visualization only)
- Project/session selector UI and sort controls
- Totals bar

**Removed:**
- `parseJSONL` — no raw JSONL parsing
- `buildTimeline` — server builds timeline now
- `extractUsage`, `sumUsages` — usage comes pre-structured from API
- `extractText`, `summarizeInput` — moved to server
- `/api/transcript` raw-text fetch + browser parsing loop
- Dedup logic (message_id dedup, best-entry selection)

**Changed:**
- `/api/transcript` fetch now receives timeline JSON directly; entries mapped to components
- Session list uses `startedAt`/`lastActive` timestamps (ISO strings) instead of `mtime` (unix float)
- `costUsd` for sessions comes from DB (no client-side recomputation)

## Out of Scope

- New analytics views (cost trends, tool charts) — future work
- Authentication or multi-user support
- Production deployment (local dev tool only)

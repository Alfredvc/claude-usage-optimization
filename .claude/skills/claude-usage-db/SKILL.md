---
name: claude-usage-db
description: Query and analyze the project's DuckDB database of ingested Claude Code transcripts (`transcripts.duckdb`). Use this skill whenever the user asks anything about querying, analyzing, summarizing, or exploring the transcripts database — costs, token usage, tool calls, sessions, subagents, model breakdowns, time-series usage, or schema introspection. Trigger on phrases like "query the duckdb", "how much did I spend", "analyze my Claude usage", "look at the transcripts db", "find sessions where…", "tool usage stats", "total cost", or any SQL-shaped question against the ingested transcripts. The schema has non-obvious billing-safety pitfalls (raw `assistant_entries` overcounts cost ~2x); this skill is essential to avoid wrong numbers.
---

# Querying the Claude transcripts DuckDB

This database (`transcripts.duckdb` at the repo root, ~4 GB) is produced by the Rust `ingest` binary parsing Claude Code JSONL transcripts from `~/.claude/projects/`. It captures every assistant turn, user turn, tool call, hook event, and metadata record across all sessions.

The schema has **one critical billing pitfall** and many subtle relational shapes. Read this skill in full before running queries.

---

## The #1 footgun: cost / token aggregation

**Never `SUM` columns on raw `assistant_entries`.** A single streaming response writes one JSONL entry per content block (thinking + text + tool_use → 3 entries) and all share the same `message_id` and the same `usage` figures. Summing raw overcounts by ~2.16× on real data.

**Always use the `assistant_entries_deduped` view** for any aggregation of:

- `cost_usd`
- `input_tokens`, `output_tokens`
- `cache_creation_input_tokens`, `cache_read_input_tokens`
- `cache_creation_5m`, `cache_creation_1h`

The view dedups by `(file_path, message_id)`, picking the authoritative row (one with `stop_reason` set, then max `output_tokens`, then min `entry_id`).

**Also filter `WHERE message_id IS NOT NULL`** for cost queries — NULL-id rows are synthetic error messages (`is_api_error_message = true`, `model = '<synthetic>'`) that were never billed.

```sql
-- Total cost across all transcripts
SELECT ROUND(SUM(cost_usd), 2) AS total_usd
FROM assistant_entries_deduped
WHERE message_id IS NOT NULL;
```

---

## Step zero: read the DB's own documentation

This DB is heavily self-documented via DuckDB `COMMENT ON` metadata. **Before guessing schema or making assumptions, query the comments.** They encode FK relationships, billing safety hints, and semantic notes that aren't visible in column names.

```sql
-- All tables and views
SHOW TABLES;
SELECT view_name, comment FROM duckdb_views() WHERE NOT internal;

-- Columns + comments for a specific table
SELECT column_name, data_type, comment
FROM duckdb_columns()
WHERE table_name = 'assistant_entries' AND comment IS NOT NULL;

-- Find every column with a billing warning
SELECT table_name, column_name, comment
FROM duckdb_columns()
WHERE comment LIKE '%DO NOT SUM%' OR comment LIKE '%billing%';

-- Discover FK relationships (encoded as "→ table(col)" or "~ table(col)")
SELECT table_name, column_name, comment
FROM duckdb_columns()
WHERE comment LIKE '%→%' OR comment LIKE '%~%';
```

Comment notation:
- `→ table(col)` — hard FK, every row has a match (enforced by ingest order)
- `~ table(col)` — soft FK, target row may be missing (e.g. unknown model not in `model_pricing`)
- `⚠ DO NOT SUM raw` — billing-unsafe column, use the deduped view

---

## Schema model

Three layers:

### 1. Root tables (one row per real-world entity)

| Table | PK | What |
|-------|-----|------|
| `transcripts` | `file_path` | one row per `.jsonl` file (session OR subagent run) |
| `entries` | `entry_id` (BIGINT seq) | one row per JSONL line — the universal join key |
| `model_pricing` | `model` | per-model USD/Mtok rates (input, output, cache 5m, cache 1h, cache read) |

`entries.file_path → transcripts(file_path)` joins everything to its source file.

### 2. Variant tables (1:1 with `entries` by `entry_id`)

Each `entries` row has a `type` (assistant/user/system/attachment/progress/...) and exactly one matching row in the corresponding variant table:

| `entries.type` | Variant table |
|---------------|---------------|
| `assistant` | `assistant_entries` |
| `user` | `user_entries` |
| `system` | `system_entries` |
| `attachment` | `attachment_entries` |
| `progress` | `progress_entries` |

Plus many narrow metadata variants (`permission_mode_entries`, `last_prompt_entries`, `ai_title_entries`, `summary_entries`, `pr_link_entries`, etc.) — see `SHOW TABLES`.

**Always join via `entry_id`:**
```sql
SELECT e.timestamp, ae.model, ae.cost_usd
FROM entries e
JOIN assistant_entries ae ON ae.entry_id = e.entry_id
WHERE e.session_id = '<uuid>';
```

(For cost, use `assistant_entries_deduped` instead — see footgun section.)

### 3. Child tables (1:N with `entries` by `(entry_id, position)`)

Repeated structures within an entry:

| Table | Holds |
|-------|-------|
| `user_content_blocks` | content blocks of a user message (text, tool_result, image, document) |
| `assistant_content_blocks` | content blocks of an assistant message (text, thinking, tool_use, redacted) |
| `assistant_usage_iterations` | API beta: per-iteration token decomposition (NOT for billing — see below) |
| `system_hook_infos` | hook invocations attached to a system entry |
| `attachment_diagnostics_files` | diagnostics files in an attachment entry |
| `attachment_invoked_skills` | skills invoked in an attachment entry |

### Convenience views

| View | Use for |
|------|---------|
| `assistant_entries_deduped` | **billing-safe** cost/token aggregation (see footgun) |
| `tool_uses` | flat tool-call rows pulled from `assistant_content_blocks` (block_type='tool_use') with extracted `effective_path`, `file_ext`, `input_command`, etc. |

---

## Sidechains and subagents

Subagent runs (Task tool, Explore agent, etc.) write to **separate `.jsonl` files** under `<session>/subagents/agent-<id>.jsonl`. The ingest treats them as their own transcripts:

- `transcripts.is_subagent` flag
- `transcripts.parent_session_id` → main session
- `transcripts.agent_id` → unique agent run id

To attribute subagent costs to a parent session:

```sql
WITH sub AS (
  SELECT t.parent_session_id AS session_id,
         SUM(d.cost_usd) AS subagent_cost
  FROM transcripts t
  JOIN entries e ON e.file_path = t.file_path
  JOIN assistant_entries_deduped d ON d.entry_id = e.entry_id
  WHERE t.is_subagent AND d.message_id IS NOT NULL
  GROUP BY 1),
main AS (
  SELECT t.session_id,
         SUM(d.cost_usd) AS main_cost
  FROM transcripts t
  JOIN entries e ON e.file_path = t.file_path
  JOIN assistant_entries_deduped d ON d.entry_id = e.entry_id
  WHERE NOT t.is_subagent AND d.message_id IS NOT NULL
  GROUP BY 1)
SELECT main.session_id,
       main.main_cost,
       COALESCE(sub.subagent_cost, 0) AS subagent_cost,
       main.main_cost + COALESCE(sub.subagent_cost, 0) AS total_cost
FROM main LEFT JOIN sub USING (session_id)
ORDER BY total_cost DESC LIMIT 20;
```

`entries.is_sidechain` may also appear within a main session JSONL for edge cases — typically safe to ignore for cost (already counted separately if it has an `agentId`).

---

## JSON columns

Polymorphic / variable-shape data is stored as DuckDB `JSON`. Query with `json_extract`, `json_extract_string`, `->`, `->>`:

| Column | Shape |
|--------|-------|
| `assistant_entries.iterations` | array of `{input_tokens, output_tokens, cache_*, type}` — API beta decomposition |
| `assistant_content_blocks.tool_input` | per-tool input object (varies by tool) |
| `user_content_blocks.tool_use_result` | tool result payload |
| `entries.raw_json` | full original JSONL line (escape hatch when nothing else fits) |

**`iterations` is NOT for billing.** It's a decomposition — top-level `usage` is always the authoritative total. When only one iteration, `iterations[0]` == top-level. The Advisor server-tool (beta) triggers multi-iteration responses where top-level is the aggregate. Never `SUM` iteration elements as a substitute for top-level tokens.

If you need iteration-level data, prefer the flattened child table `assistant_usage_iterations` over the JSON column.

---

## Common query recipes

### Top sessions by cost
```sql
SELECT e.session_id,
       MIN(e.timestamp) AS started_at,
       ROUND(SUM(d.cost_usd), 2) AS cost_usd
FROM assistant_entries_deduped d
JOIN entries e ON e.entry_id = d.entry_id
WHERE d.message_id IS NOT NULL
GROUP BY 1
ORDER BY cost_usd DESC LIMIT 20;
```

### Cost by model
```sql
SELECT model,
       COUNT(*) AS n_responses,
       ROUND(SUM(cost_usd), 2) AS cost_usd,
       SUM(input_tokens) AS in_tok,
       SUM(output_tokens) AS out_tok
FROM assistant_entries_deduped
WHERE message_id IS NOT NULL
GROUP BY 1 ORDER BY cost_usd DESC;
```

### Cost per day
```sql
SELECT DATE_TRUNC('day', e.timestamp) AS day,
       ROUND(SUM(d.cost_usd), 2) AS cost_usd
FROM assistant_entries_deduped d
JOIN entries e ON e.entry_id = d.entry_id
WHERE d.message_id IS NOT NULL
GROUP BY 1 ORDER BY 1;
```

### Tool usage frequency
```sql
SELECT name, COUNT(*) AS n
FROM tool_uses
GROUP BY 1 ORDER BY n DESC LIMIT 30;
```

### Bash commands run today
```sql
SELECT e.timestamp, t.input_command
FROM tool_uses t
JOIN entries e ON e.entry_id = t.entry_id
WHERE t.name = 'Bash'
  AND DATE_TRUNC('day', e.timestamp) = CURRENT_DATE
ORDER BY e.timestamp;
```

### Files most often edited
```sql
SELECT effective_path, COUNT(*) AS n
FROM tool_uses
WHERE name IN ('Edit','Write','NotebookEdit') AND effective_path IS NOT NULL
GROUP BY 1 ORDER BY n DESC LIMIT 30;
```

### Cache hit ratio per model
```sql
SELECT model,
       SUM(cache_read_input_tokens) AS cache_read,
       SUM(input_tokens) AS new_input,
       ROUND(100.0 * SUM(cache_read_input_tokens) /
             NULLIF(SUM(cache_read_input_tokens) + SUM(input_tokens), 0), 1) AS pct_cached
FROM assistant_entries_deduped
WHERE message_id IS NOT NULL
GROUP BY 1;
```

---

## How to actually run queries

The DB is a single file. From the repo root:

```bash
# Interactive
duckdb transcripts.duckdb

# One-shot
duckdb transcripts.duckdb "SELECT COUNT(*) FROM entries;"

# Pipe a query file
duckdb transcripts.duckdb < query.sql
```

DuckDB CLI is at `/opt/homebrew/bin/duckdb` on this machine. If unavailable, the Python `duckdb` package or any DuckDB-compatible tool works — same SQL.

---

## What NOT to do

| Mistake | Why wrong | Do instead |
|---------|-----------|-----------|
| `SUM(cost_usd)` on `assistant_entries` | overcounts ~2× (multi-block responses share usage) | use `assistant_entries_deduped`, filter `message_id IS NOT NULL` |
| Sum elements of `iterations` JSON for tokens | iterations is a decomposition, not an alternative billing source | use top-level `input_tokens`/`output_tokens` from deduped view |
| Assume `message_id` is unique within a file | one streaming response writes N rows sharing it | dedup by `(file_path, message_id)` (already done by the view) |
| Treat synthetic error rows as billable | `model = '<synthetic>'`, never hit the API | filter `WHERE message_id IS NOT NULL AND NOT is_api_error_message` |
| Count subagent activity inside the main session file | subagents live in separate JSONLs | join via `transcripts.parent_session_id` |
| Hardcode column lists | schema evolves; use `duckdb_columns()` to introspect | query the metadata first |

---

## When in doubt

1. `SHOW TABLES;` and `SELECT view_name, comment FROM duckdb_views();`
2. `SELECT column_name, data_type, comment FROM duckdb_columns() WHERE table_name = '<x>';`
3. Read `docs/cost-attribution.md` in the repo for the full story on billing dedup.
4. Read `FINDINGS.md` for the on-disk JSONL format that produced this DB.

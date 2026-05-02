# Spec: Support new `last-prompt` (leafUuid) format

**Date:** 2026-05-02
**Status:** Draft (rev 2 — empirical findings + reviewer feedback applied)

## Problem

`cct ingest` aborts with:

```
parse: typed parse: missing field `lastPrompt`
error: parse failure in /Users/alfredvc/.claude/projects/-Users-alfredvc-src-agentfiles/2c881fe2-dc9b-4ab6-9718-a39dcc307849.jsonl
```

Claude Code emits two formats for `"type":"last-prompt"` JSONL entries:

**Old (still present in many files):**
```json
{"type":"last-prompt","lastPrompt":"hi","sessionId":"..."}
```

**New (rejected by current parser):**
```json
{"type":"last-prompt","leafUuid":"ccd8140d-...","sessionId":"2c881..."}
```

`crates/claude-code-transcripts/src/types.rs:1015-1020` declares `LastPromptEntry { last_prompt: String, session_id: String }` with `last_prompt` required. serde fails the line, the parser aborts the file, and `cct ingest` exits non-zero.

## Empirical findings (verified on `~/.claude/projects/`)

Sampled 50 new-format `last-prompt` entries. The leaf entry referenced by `leafUuid` is the conversation-tree leaf at session-save time. It is **not** a pointer to the prompt text. Distribution of leaf entry types:

| Leaf entry type | Count | Carries prompt text? |
|---|---|---|
| `attachment` (hook output) | 15 | No |
| `user` (tool_result wrapper) | 10 | No |
| `assistant` (tool_use) | 4 | No |
| `user` (text block) | 1 | Sometimes |

The prompt text is **not stored in the new-format `last-prompt` entry, nor in the leaf entry itself**. It lives in the most recent ancestor of the leaf that is a user entry with text content. Walking the chain via `parentUuid` (falling back to `logicalParentUuid` at compact boundaries) until reaching a user entry with text content resolves 46/50 cases (92%).

The 4/50 unresolved cases (8%) all share one shape: the leaf is a `SessionStart` hook-output attachment with `parentUuid = null` and no `logicalParentUuid`. The session was opened (SessionStart hook fired) and saved/closed before the user typed any prompt. There is no last prompt to recover — `NULL` is the correct value for these rows.

Resolution rate against "sessions where a prompt was actually typed" is therefore effectively 100%.

## Goals

1. `cct ingest` succeeds on transcripts containing either format (or both within a single file).
2. The physical schema **faithfully represents the JSONL** — no interpretation in the storage layer.
3. A view exposes resolved prompt text so `SELECT last_prompt FROM last_prompt_entries` continues to return human-readable strings without query rewrites.
4. No skill files require changes.

## Non-goals

- Changing the JSONL on-disk format.
- Reconstructing prompt text in cases where the source data does not contain it (the 8% above).
- In-place migration of pre-existing DuckDB files (`cct ingest` rebuilds, see §5).

## Design

### 1. Type changes (faithful to JSONL)

`crates/claude-code-transcripts/src/types.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastPromptEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leaf_uuid: Option<String>,
    pub session_id: String,
}
```

Both `last_prompt` and `leaf_uuid` are `Option<String>`. The parser does not enforce that one of them is present; an entry with neither is still ingested with both NULL (forward-compatible with future format variants).

### 2. Schema: faithful table + ergonomic view

`crates/claude-code-transcripts-ingest/src/schema.rs`:

The physical table stores exactly what the JSONL contains. The user-facing name `last_prompt_entries` becomes a view that resolves prompt text.

```sql
CREATE TABLE IF NOT EXISTS last_prompt_entries_raw (
    entry_id    BIGINT,
    last_prompt TEXT,   -- inline text from old-format rows; NULL for new format
    leaf_uuid   TEXT,   -- pointer to conversation-tree leaf; NULL for old format
    session_id  TEXT
);

CREATE OR REPLACE VIEW last_prompt_entries AS
WITH RECURSIVE chain(start_entry_id, cur_entry_id, depth) AS (
    -- Seed: each new-format raw row, joined to its leaf entry
    SELECT
        r.entry_id        AS start_entry_id,
        e.entry_id        AS cur_entry_id,
        0                 AS depth
    FROM last_prompt_entries_raw r
    JOIN entries e
      ON e.uuid = r.leaf_uuid
     AND e.session_id = r.session_id
    WHERE r.last_prompt IS NULL
      AND r.leaf_uuid   IS NOT NULL

    UNION ALL

    -- Walk: parent_uuid first, then logical_parent_uuid at compact boundaries
    SELECT
        c.start_entry_id,
        p.entry_id,
        c.depth + 1
    FROM chain c
    JOIN entries cur ON cur.entry_id = c.cur_entry_id
    JOIN entries p
      ON (p.uuid = cur.parent_uuid OR
          (cur.parent_uuid IS NULL AND p.uuid = cur.logical_parent_uuid))
     AND p.session_id = cur.session_id
    WHERE c.depth < 500
      AND NOT EXISTS (   -- stop at first user-text ancestor
            SELECT 1
            FROM user_entries ue_stop
            WHERE ue_stop.entry_id = c.cur_entry_id
              AND (ue_stop.message_content_text IS NOT NULL
                   OR EXISTS (SELECT 1 FROM user_content_blocks b
                              WHERE b.entry_id = ue_stop.entry_id
                                AND b.block_type = 'text'
                                AND b.text IS NOT NULL))
      )
),
resolved AS (
    SELECT
        c.start_entry_id,
        COALESCE(
            ue.message_content_text,
            (SELECT string_agg(b.text, '' ORDER BY b.position)
             FROM user_content_blocks b
             WHERE b.entry_id = c.cur_entry_id
               AND b.block_type = 'text')
        ) AS resolved_text,
        ROW_NUMBER() OVER (PARTITION BY c.start_entry_id ORDER BY c.depth ASC) AS rn
    FROM chain c
    LEFT JOIN user_entries ue ON ue.entry_id = c.cur_entry_id
    WHERE EXISTS (
        SELECT 1 FROM user_entries ue2
        WHERE ue2.entry_id = c.cur_entry_id
          AND (ue2.message_content_text IS NOT NULL
               OR EXISTS (SELECT 1 FROM user_content_blocks b2
                          WHERE b2.entry_id = ue2.entry_id
                            AND b2.block_type = 'text'
                            AND b2.text IS NOT NULL))
    )
)
SELECT
    r.entry_id,
    COALESCE(r.last_prompt, res.resolved_text) AS last_prompt,
    r.session_id
FROM last_prompt_entries_raw r
LEFT JOIN resolved res
       ON res.start_entry_id = r.entry_id
      AND res.rn = 1;
```

Resolution semantics:
1. Old-format rows: `last_prompt` is the inline text. View returns it directly.
2. New-format rows: chain walks `parent_uuid` (then `logical_parent_uuid` if NULL) up from the leaf, stops at the first user entry with text content, returns that text.
3. Unresolvable new-format rows (leaf has no user-text ancestor): view returns NULL. This is the documented "no prompt was typed" case.

Notes on the SQL:
- Both branches of the recursive step need a single, well-defined parent. The expression `(p.uuid = cur.parent_uuid OR (cur.parent_uuid IS NULL AND p.uuid = cur.logical_parent_uuid))` selects exactly one ancestor per step provided UUIDs are unique within a session (which they are — UUIDs in a single transcript are guaranteed unique by Claude Code).
- The stop condition ("don't recurse past the first user-text ancestor") sits in the `WHERE NOT EXISTS (...)` clause of the recursive arm, so the chain CTE terminates as soon as a hit is found.
- DuckDB supports `WITH RECURSIVE` and `string_agg(expr, sep ORDER BY ...)` — both verified during spec authoring against current DuckDB docs. (To be re-confirmed at implementation time using `duckdb --version` of the bundled `libduckdb-sys`.)
- The 1:1 invariant of `last_prompt_entries.entry_id` is preserved: `last_prompt_entries_raw(entry_id)` has the existing unique index, and the LEFT JOIN to `resolved` is filtered to `rn = 1`. A comment on the view documents this.

The unique index `uq_last_prompt_entries_pk` (currently on `last_prompt_entries(entry_id)`) moves to `last_prompt_entries_raw(entry_id)`. The `COMMENT ON COLUMN last_prompt_entries.entry_id` moves to `last_prompt_entries_raw.entry_id`. A new comment on the view documents resolution semantics and the 1:1 contract.

### 3. Parser changes

`crates/claude-code-transcripts-ingest/src/parse.rs:468-474`:

```rust
Entry::LastPrompt(x) => Ok((
    Some((
        "last_prompt_entries_raw",
        vec![
            Value::Null,
            x.last_prompt.as_deref().map(s_str).unwrap_or(Value::Null),
            x.leaf_uuid.as_deref().map(s_str).unwrap_or(Value::Null),
            s_str(&x.session_id),
        ],
    )),
    vec![],
)),
```

Insert target changes from `last_prompt_entries` to `last_prompt_entries_raw` and gains the `leaf_uuid` column.

### 4. Demo build script

`scripts/demo/build.sh:409-411` selects from `src.last_prompt_entries` and applies `fakepara`. DuckDB allows `CREATE TABLE … AS SELECT … FROM <view>`, so the demo continues to work.

The view's `last_prompt` column may resolve via JOINs to source rows whose `entry_id` is not in `keep_entries`. This is safe because the demo selects from the unfiltered `src` database — the resolution happens against the full source, not the filtered output. `fakepara` then scrubs the resolved string, which is the desired outcome (the recovered prompt text gets anonymized, not just the literal stored value).

Add a brief comment in `build.sh` near line 409 documenting that the view-resolved text is what gets scrubbed.

### 5. Migration

Confirmed: `crates/claude-code-transcripts-ingest/src/run.rs:46` calls `remove_db_files` unconditionally before opening the connection. Every `cct ingest` is a full rebuild from JSONL. There is no incremental ingest mode in this codebase.

Therefore the schema bootstrap always starts from an empty DB, and no `DROP TABLE IF EXISTS last_prompt_entries` migration step is needed. The previous draft of this spec called for one — removed.

### 6. Schema header docstring

The schema docstring at `crates/.../schema.rs:1-7` currently describes only the table layout. It should be extended with a note that `last_prompt_entries` is a view over `last_prompt_entries_raw` plus a recursive resolution; this is the only such pattern in the schema today, so it warrants a brief mention to avoid surprising future readers.

## Skill / consumer impact

Surveyed every consumer of `last_prompt` and `last_prompt_entries`:

| Consumer | File | Change required |
|---|---|---|
| Type | `crates/.../types.rs:1015-1020` | Yes — `Option<String>` + `leaf_uuid` |
| Parser | `crates/.../parse.rs:468-474` | Yes — table name + columns |
| Schema | `crates/.../schema.rs:295-299` | Yes — table → raw + view |
| Schema indexes | `crates/.../schema.rs:538` | Yes — index target name |
| Schema comments | `crates/.../schema.rs:609` | Yes — comment target + new view comment |
| Schema header | `crates/.../schema.rs:1-7` | Yes — note view pattern |
| Demo build | `scripts/demo/build.sh:409-411` | Comment only (no SQL change) |
| Skill (claude-usage-db) | `skills/claude-usage-db/SKILL.md:64` | No — only mentions table name in variant list |
| Skill (optimize-usage) | `skills/optimize-usage/**` | No — no references |
| Design doc (historical) | `docs/superpowers/specs/2026-04-15-…` | No — names table in variant list, accurate either way |

Verification procedure for "no skill changes": grep for `last_prompt_entries` and `last_prompt` across `skills/` and `docs/`; for each occurrence, confirm it is documentation prose or a SQL example that still produces the same result against the view. The view returns the same column tuple `(entry_id, last_prompt, session_id)` as the old physical table.

## Behavior matrix

| Source row | `last_prompt_entries_raw.last_prompt` | `…raw.leaf_uuid` | View `last_prompt` |
|---|---|---|---|
| Old format | `"hi"` | NULL | `"hi"` |
| New format, leaf has user-text ancestor | NULL | `"u1"` | resolved text from chain walk |
| New format, leaf is SessionStart hook attachment with no ancestors | NULL | `"u1"` | NULL (no prompt was typed) |
| New format, leaf UUID dangling (not in file) | NULL | `"u1"` | NULL |
| Future format, neither field | NULL | NULL | NULL |

NULL is a new possible value for `last_prompt`. The old physical column was always populated. Skills surveyed do not depend on non-NULL.

## Risks

- **Recursive CTE performance**: `last_prompt_entries_raw` is small (one row per session-save). The recursive JOIN on `entries` walks at most depth ~500 (capped). For 8872 transcript files this is negligible at query time. If profiling shows otherwise, materialize via a post-ingest UPDATE pass — design changeable without altering the public surface.
- **DuckDB version compatibility**: spec assumes `WITH RECURSIVE` + `string_agg(... ORDER BY ...)` are supported. DuckDB has supported both for several major versions; verify against the bundled `libduckdb-sys` at implementation time.
- **Cross-session UUID collision**: the recursive JOIN keys on `(uuid, session_id)`, defending against the theoretical case of UUID collision across sessions.
- **Unicode / large prompts**: resolved text can be very large (multi-KB user messages). DuckDB `TEXT` has no length limit; downstream skills already handle this.

## Acceptance criteria

1. `cct ingest` completes without error on the user's `~/.claude/projects/` directory containing both old- and new-format `last-prompt` rows.
2. **Old-format resolution**: given a JSONL fixture with `{"type":"last-prompt","lastPrompt":"hello world","sessionId":"S1"}`, `SELECT last_prompt FROM last_prompt_entries WHERE session_id = 'S1'` returns exactly `"hello world"`.
3. **New-format resolution**: given a JSONL fixture with a `last-prompt` row whose `leafUuid` chains (via `parentUuid`) to a user entry with text content `"reach me"`, the same query returns exactly `"reach me"`.
4. **Compact boundary resolution**: given a fixture where the chain crosses a compact boundary (`parentUuid = NULL`, `logicalParentUuid` set), the query still returns the upstream user-text content.
5. **Unresolvable case**: given a fixture where the leaf is a SessionStart hook attachment with no ancestors, the query returns NULL (not an error, not an empty string).
6. **Row count parity**: `SELECT COUNT(*) FROM last_prompt_entries` equals `SELECT COUNT(*) FROM last_prompt_entries_raw`.
7. **Resolution rate on real corpus**: on the user's `~/.claude/projects/`, the fraction of view rows where `last_prompt IS NOT NULL` is at least 90%. (Empirical baseline: 92% on a 50-row sample; allow a small safety margin for variance.)
8. **Demo build**: `scripts/demo/build.sh` runs successfully against a database built with the new schema and produces a `last_prompt_entries` table whose values are scrubbed strings.
9. **Skill survey**: `grep -rn 'last_prompt' skills/ docs/` produces no occurrence that breaks; each is either prose or a query that still resolves against the view. Manually run any SQL example that appears.
10. **No new clippy / build warnings** introduced by the type or parser changes.

## Out of scope

- Backfilling old DuckDBs without re-ingest (no incremental mode exists).
- Reconstructing prompt text for the documented 8% no-prompt-typed case.
- Exposing `leaf_uuid` in the view (consumers who need the chain pointer can read `last_prompt_entries_raw` directly).
- Altering the JSONL on-disk format.

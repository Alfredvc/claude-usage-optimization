# Spec: Support new `last-prompt` (leafUuid) format

**Date:** 2026-05-02
**Status:** Draft

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

The new format omits `lastPrompt` and adds `leafUuid`, a pointer to an existing entry's `uuid` whose message text is the prompt content.

`crates/claude-code-transcripts/src/types.rs:1015-1020` declares `LastPromptEntry { last_prompt: String, session_id: String }` with `last_prompt` required. serde fails the line, the parser aborts the file, and `cct ingest` exits non-zero.

## Goals

1. `cct ingest` succeeds on transcripts containing either format (or both).
2. Existing user queries against `last_prompt_entries.last_prompt` continue returning prompt text without modification.
3. No skill files require changes.
4. Both formats are preserved losslessly so future consumers can distinguish them if needed.

## Non-goals

- Changing the JSONL on-disk format.
- Backfilling resolved text into a physical column at ingest time.
- Migrating existing DuckDB databases in place — next ingest rebuilds.

## Design

### 1. Type changes

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

Both `last_prompt` and `leaf_uuid` are `Option<String>`. At least one is expected to be present, but the parser does not enforce it — an entry with neither populates both as NULL and is still ingested (forward-compatible with future format variants).

### 2. Schema changes

`crates/claude-code-transcripts-ingest/src/schema.rs`:

Rename the physical table to `last_prompt_entries_raw` and expose the user-facing name as a view.

```sql
CREATE TABLE IF NOT EXISTS last_prompt_entries_raw (
    entry_id    BIGINT,
    last_prompt TEXT,
    leaf_uuid   TEXT,
    session_id  TEXT
);

CREATE OR REPLACE VIEW last_prompt_entries AS
SELECT
    r.entry_id,
    COALESCE(
        r.last_prompt,
        ue.message_content_text,
        (SELECT string_agg(b.text, '' ORDER BY b.position)
         FROM user_content_blocks b
         WHERE b.entry_id = e.entry_id AND b.block_type = 'text')
    ) AS last_prompt,
    r.session_id
FROM last_prompt_entries_raw r
LEFT JOIN entries e
    ON e.uuid = r.leaf_uuid
   AND e.session_id = r.session_id
LEFT JOIN user_entries ue
    ON ue.entry_id = e.entry_id;
```

The view exposes `(entry_id, last_prompt, session_id)` — identical column shape to the previous physical table, so existing queries are unaffected.

Resolution order for `last_prompt`:
1. Inline `last_prompt` (old-format rows).
2. `user_entries.message_content_text` of the entry referenced by `leaf_uuid` (covers user prompts stored as plain text).
3. Concatenated text from `user_content_blocks` for entries with multi-block message bodies.

The JOIN keys on `(uuid, session_id)` rather than `uuid` alone to avoid cross-session collisions if a UUID ever repeats.

If `leaf_uuid` references an entry not in the same file (or not yet ingested), the COALESCE produces NULL. This is documented as expected behavior.

The unique index `uq_last_prompt_entries_pk` (currently on `last_prompt_entries(entry_id)`) moves to `last_prompt_entries_raw(entry_id)`. The `COMMENT ON COLUMN last_prompt_entries.entry_id` moves to `last_prompt_entries_raw.entry_id`. A new comment on the view documents the resolution semantics.

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

`scripts/demo/build.sh:409-411` selects from `src.last_prompt_entries` and applies `fakepara`. The view is queryable; DuckDB allows `CREATE TABLE … AS SELECT … FROM <view>`. The demo script keeps working unchanged. The destination database in the demo build is materialized as a table, not a view — no change needed.

### 5. Migration

`cct ingest` rebuilds the DuckDB from JSONL on each run (verify in `run.rs` during implementation; if it does not, the rebuild path is the recommended UX for this change). The schema bootstrap runs `DROP TABLE IF EXISTS last_prompt_entries` before creating the view, since DuckDB rejects `CREATE OR REPLACE VIEW` against an object that already exists as a table. The new `CREATE TABLE IF NOT EXISTS last_prompt_entries_raw` then sits alongside.

Users who run incremental ingest against a pre-existing DuckDB get the bootstrap drop-and-recreate automatically. Document the expectation in the release notes / commit message.

## Skill / consumer impact

Surveyed every consumer of `last_prompt` and `last_prompt_entries` under `crates/`, `skills/`, `~/.claude/skills/`, `docs/`, `scripts/`:

| Consumer | File | Change required |
|---|---|---|
| Parser | `crates/.../parse.rs:468` | Yes — table name + column |
| Schema | `crates/.../schema.rs:295` | Yes — table → raw + view |
| Schema indexes | `crates/.../schema.rs:538` | Yes — index target |
| Schema comments | `crates/.../schema.rs:609` | Yes — comment target |
| Type | `crates/.../types.rs:1015` | Yes — fields optional + leaf_uuid |
| Demo build | `scripts/demo/build.sh:409` | No |
| Skill (claude-usage-db) | `skills/claude-usage-db/SKILL.md:64` | No (only mentions table name in variant list) |
| Skill (optimize-usage) | `skills/optimize-usage/**` | No (no references) |
| Design doc | `docs/superpowers/specs/2026-04-15-…` | No (historical, names table in variant list) |

User-issued queries of the form `SELECT last_prompt FROM last_prompt_entries WHERE …` continue to work because the view preserves column shape.

## Behavior matrix

| Source row | `last_prompt` (raw) | `leaf_uuid` (raw) | View `last_prompt` |
|---|---|---|---|
| Old format | `"hi"` | NULL | `"hi"` |
| New format, leaf in file, single text block | NULL | `"u1"` | resolved from `user_entries.message_content_text` |
| New format, leaf in file, multi-block | NULL | `"u1"` | resolved from `user_content_blocks` concatenation |
| New format, leaf missing / not yet ingested | NULL | `"u1"` | NULL |
| Future format, neither field | NULL | NULL | NULL |

NULL is a new possible value for `last_prompt`. Old behavior never produced NULL. No skill query depends on non-null, but downstream code that does will see NULL for unresolvable references — this is the documented behavior.

## Risks

- **Leaf UUID points at an assistant entry**: the JOIN against `user_entries` would miss. Mitigation: spec assumes user-entry references; if assistant references appear in practice, extend the COALESCE chain to include `assistant_entries`/`assistant_content_blocks`. Verify by sampling JSONL before implementation.
- **View performance**: each query against `last_prompt_entries` triggers two JOINs and a correlated subquery for the multi-block branch. Acceptable — the table is small (one row per session-prompt) and queries are interactive, not hot-path. If profiling shows otherwise, materialize as a table on ingest.
- **`string_agg` ordering**: DuckDB supports `string_agg(expr, sep ORDER BY …)`. Verify syntax during implementation; fall back to a subquery if needed.

## Out of scope

- Backfilling old DuckDBs without re-ingest.
- Exposing `leaf_uuid` in the view (consumers currently don't need it; `last_prompt_entries_raw` is available for power users).
- Resolving cross-file `leaf_uuid` references (not observed in current data).

## Acceptance criteria

1. `cct ingest` completes without error on a transcript directory containing both old and new `last-prompt` formats.
2. `SELECT last_prompt FROM last_prompt_entries WHERE session_id = '<old>'` returns the inline string.
3. `SELECT last_prompt FROM last_prompt_entries WHERE session_id = '<new>'` returns the resolved text from the referenced user entry.
4. `SELECT COUNT(*) FROM last_prompt_entries` matches `SELECT COUNT(*) FROM last_prompt_entries_raw`.
5. `scripts/demo/build.sh` runs successfully against a database built with the new schema.
6. No skill or doc file references break.

# Spec: Support new `last-prompt` (leafUuid) format

**Date:** 2026-05-02
**Status:** Draft (rev 3 — empirical pivot to faithful pass-through)

## Problem

`cct ingest` aborts with:

```
parse: typed parse: missing field `lastPrompt`
error: parse failure in /Users/alfredvc/.claude/projects/-Users-alfredvc-src-agentfiles/2c881fe2-dc9b-4ab6-9718-a39dcc307849.jsonl
```

Claude Code emits two formats for `"type":"last-prompt"` JSONL entries:

```json
{"type":"last-prompt","lastPrompt":"hi","sessionId":"..."}
{"type":"last-prompt","leafUuid":"ccd8140d-...","sessionId":"2c881..."}
```

`crates/claude-code-transcripts/src/types.rs:1015-1020` declares `LastPromptEntry { last_prompt: String, session_id: String }` with `last_prompt` required. serde fails new-format lines on missing field, the parser aborts the whole file, ingest exits non-zero.

## Empirical findings

Sampled the user's `~/.claude/projects/` corpus directly.

**1. `leafUuid` semantics.** In 50 sampled new-format rows, the leaf entry referenced by `leafUuid` is the conversation-tree leaf at session-save, not a pointer to the prompt text. Distribution of leaf entry types:

| Leaf entry type | Count |
|---|---|
| `attachment` (hook output) | 15 |
| `user` (`tool_result` blocks only) | 10 |
| `assistant` (`tool_use` blocks only) | 4 |
| `user` (text block) | 1 |

The user's typed prompt text is **not stored anywhere in the new-format `last-prompt` entry**. The leaf's own content is rarely text; ancestor user-text entries exist but contain the *expanded* message body sent to the model, not the literal typed input.

**2. Old vs. new format are not interchangeable strings.** Sampled old-format `lastPrompt` values store the literal user-typed text, including unexpanded slash commands:

```
"/agentfiles:research For some reason i get \"Please run /login · API Error: 401 …"
```

Walking the parent chain to the corresponding user entry yields the *expanded* form:

```
"<command-message>agentfiles:research</command-message>\n<command-name>…\nBase directory for this skill: /Users/alfredvc/src/agentfiles/skills/research\n\nARGUMENTS: For some reason i get …"
```

These represent different things. There is no faithful function from new-format JSONL data to a string equivalent to old-format `lastPrompt`. The literal typed input is no longer in the transcript.

**3. UUID uniqueness within `session_id` does not hold.** On the user's current `transcripts.duckdb`:

```
647031 entries; 22076 rows belong to a (uuid, session_id) pair that appears more than once
12608 distinct duplicated pairs
```

Resumed-session JSONL files replay overlapping UUID prefixes under the same `session_id`. Any chain-walk design must handle this.

**4. DuckDB version.** Pinned at `1.10502.0` via `libduckdb-sys` in `Cargo.lock` (workspace-bundled).

## Design principle

Per project mandate: **faithfully and correctly represent the transcripts in SQL**. Do not synthesize values that the JSONL does not contain. The earlier draft proposed a recursive view that fabricated a `last_prompt` value from the expanded message body of an ancestor entry; finding (2) shows that value is *not* what old-format `lastPrompt` stored. Removing it.

## Goals

1. `cct ingest` succeeds on transcripts containing either format (or both within a single file).
2. The `last_prompt_entries` table contains exactly the fields Claude Code emits — nothing more, nothing less.
3. Existing queries `SELECT last_prompt FROM last_prompt_entries` continue to parse and return inline text where Claude Code stored it, NULL where it did not.
4. Power users who want to follow `leaf_uuid` to a specific entry can do so directly against `entries.uuid`.

## Non-goals

- Reconstructing typed user input that Claude Code stopped storing.
- Synthesizing `last_prompt` text via chain-walk, message-body extraction, or slash-command de-expansion. (None of these recover the original input faithfully.)
- Changing the on-disk JSONL format.
- In-place migration of pre-existing DuckDBs (`cct ingest` rebuilds from JSONL on every run — confirmed at `crates/claude-code-transcripts-ingest/src/run.rs:46`).

## Design

### 1. Type changes

`crates/claude-code-transcripts/src/types.rs:1015-1020`:

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

Both fields optional. Serde's `rename_all = "camelCase"` maps them to `lastPrompt` and `leafUuid` respectively. An entry with neither field still parses; both columns become NULL in the DB row, which is faithful. This tolerates new optional fields being added in future Claude Code versions, but does **not** automatically interpret a future required discriminator (e.g. a `kind` field): such a change would still need explicit type-level support.

The `Entry::LastPrompt` variant on the top-level `Entry` enum at `types.rs:31-32` does not change.

### 2. Schema changes

`crates/claude-code-transcripts-ingest/src/schema.rs:295-299`:

```sql
CREATE TABLE IF NOT EXISTS last_prompt_entries (
    entry_id    BIGINT,
    leaf_uuid   TEXT,    -- pointer to entries.uuid for new-format rows; NULL for old format
    last_prompt TEXT,    -- inline text from old-format rows; NULL for new format
    session_id  TEXT
);
```

Adds `leaf_uuid TEXT`. Column order mirrors the existing `summary_entries(entry_id, leaf_uuid, summary, session_id)` precedent at `schema.rs:337-342`, which already uses `leaf_uuid` in the same position. The existing DDL already declared `last_prompt TEXT` without `NOT NULL` (verified at `schema.rs:297`); only the Rust struct enforced non-null at parse time. No `ALTER COLUMN` is needed. No view is created.

The unique index at `schema.rs:538` (`uq_last_prompt_entries_pk ON last_prompt_entries(entry_id)`) is unchanged.

The column comment at `schema.rs:609` (`COMMENT ON COLUMN last_prompt_entries.entry_id IS '→ entries(entry_id)'`) is unchanged. Two new comments are added:

```sql
COMMENT ON COLUMN last_prompt_entries.last_prompt IS 'Inline literal user-typed prompt text (old Claude Code format). NULL for new-format rows; new format stores leaf_uuid only.';
COMMENT ON COLUMN last_prompt_entries.leaf_uuid   IS '~ entries(uuid). Conversation-tree leaf at session-save (new Claude Code format). NULL for old-format rows. Does not point at the prompt-text entry. Soft reference: entries(uuid) is non-unique across resumed sessions, so joins are 1:N.';
```

The `~` notation marks this as a soft / non-unique reference per the schema's documented arrow legend at `schema.rs:583-586`. Using `→` would be wrong: `→` means "every row in this table has exactly one matching row in the target", and resumed-session UUID replay (~22k duplicate `(uuid, session_id)` pairs in the user's current corpus, see Empirical Findings) breaks that contract.

The schema header docstring at `schema.rs:1-7` does not need changes — there is no view, no recursive CTE, no resolution semantics. The columns speak for themselves.

### 3. Parser changes

`crates/claude-code-transcripts-ingest/src/parse.rs:468-474`:

```rust
Entry::LastPrompt(x) => {
    if x.last_prompt.is_none() && x.leaf_uuid.is_none() {
        // Observability: a `last-prompt` row with neither field signals a
        // possible Claude Code format change (e.g. renamed discriminator).
        // Surfaces in ingest's existing `unknown_variants` log so a future
        // format break is loud, not silent.
        unknown_variants.push("last-prompt:no-fields".to_string());
    }
    Ok((
        Some((
            "last_prompt_entries",
            vec![
                Value::Null,
                x.leaf_uuid.as_deref().map_or(Value::Null, s_str),
                x.last_prompt.as_deref().map_or(Value::Null, s_str),
                s_str(&x.session_id),
            ],
        )),
        vec![],
    ))
}
```

Insert path gains the `leaf_uuid` value column. Vec order matches the schema column order `(entry_id, leaf_uuid, last_prompt, session_id)`. The `Value::Null` placeholder for `entry_id` follows the existing convention for variant tables (cf. `parse.rs:464` for `permission_mode_entries`); the writer pipeline assigns the actual `entry_id` later. The DuckDB `Appender` API is positional (verified at `run.rs:21,439-440`), so vec order is the contract.

The all-fields-NULL signal piggybacks on the existing `unknown_variants: Vec<String>` that the parser already threads through (verified at `parse.rs:45,182,448`). One push per occurrence; the existing aggregation surfaces the count in the ingest summary log, giving observability for the next format break without aborting ingest.

### 4. Demo build script

`scripts/demo/build.sh:409-411` reads:

```bash
CREATE TABLE last_prompt_entries AS
SELECT entry_id, fakepara(last_prompt) AS last_prompt, session_id
FROM src.last_prompt_entries WHERE entry_id IN (SELECT entry_id FROM keep_entries);
```

Update to also project `leaf_uuid` unchanged. This is consistent with the demo's existing UUID handling: `entries.uuid` is passed through at `build.sh:136`, and `summary_entries.leaf_uuid` is passed through at `build.sh:440`. The demo's threat model already accepts UUID exposure (UUIDs allow correlation of session structure across snapshots, but no PII content). Pass-through preserves the implicit `leaf_uuid → entries(uuid)` reference within the demo DB; remapping would dangle.

```bash
CREATE TABLE last_prompt_entries AS
SELECT entry_id, fakepara(last_prompt) AS last_prompt, leaf_uuid, session_id
FROM src.last_prompt_entries WHERE entry_id IN (SELECT entry_id FROM keep_entries);
```

`fakepara` handles NULL inputs explicitly — verified at `scripts/demo/build.sh:78` (`WHEN orig IS NULL THEN NULL`). No per-row branching needed.

### 5. Migration

`crates/claude-code-transcripts-ingest/src/run.rs:46` calls `remove_db_files` unconditionally before opening the connection. Every `cct ingest` is a full rebuild. Users see the new schema after a single re-ingest.

## Behavior matrix

| JSONL row | `last_prompt_entries.last_prompt` | `last_prompt_entries.leaf_uuid` |
|---|---|---|
| `{"type":"last-prompt","lastPrompt":"hi","sessionId":"S"}` | `"hi"` | NULL |
| `{"type":"last-prompt","leafUuid":"u1","sessionId":"S"}` | NULL | `"u1"` |
| `{"type":"last-prompt","lastPrompt":"hi","leafUuid":"u1","sessionId":"S"}` (hypothetical future) | `"hi"` | `"u1"` |
| `{"type":"last-prompt","sessionId":"S"}` (hypothetical future) | NULL | NULL |

NULL is now a possible value for `last_prompt`. The old physical column was always non-NULL because the Rust type required it; downstream queries that previously assumed non-NULL will see NULL on new-format rows.

## Skill / consumer impact

| Consumer | File | Change required |
|---|---|---|
| Type | `crates/claude-code-transcripts/src/types.rs:1015-1020` | Yes — `Option<String>` + `leaf_uuid` |
| Parser | `crates/.../parse.rs:468-474` | Yes — pass `leaf_uuid` value |
| Schema | `crates/.../schema.rs:295-299` | Yes — add `leaf_uuid TEXT` column |
| Schema column comments | `crates/.../schema.rs:609` | Yes — add two new `COMMENT ON COLUMN` |
| Demo build | `scripts/demo/build.sh:409-411` | Yes — project `leaf_uuid` |
| Skill (claude-usage-db) | `skills/claude-usage-db/SKILL.md:64` | Yes — add a one-line note that `last_prompt` is NULL for new-format rows and `leaf_uuid` joins to `entries(uuid)` |
| Skill (optimize-usage) | `skills/optimize-usage/**` | No (no references) |

Verification procedure for "no skill changes": `grep -rn 'last_prompt' skills/ docs/` and confirm each occurrence is documentation prose or a SQL example whose result shape is preserved (column tuple `(entry_id, last_prompt, session_id)` is still a valid `SELECT` against the table; the table just has one extra column that the existing queries don't reference).

## Risks

- **Ad-hoc queries assume non-NULL `last_prompt`.** Surveyed: no skill SQL references the column. The risk is limited to user-written ad-hoc queries (e.g. `WHERE last_prompt LIKE '%foo%'` silently drops new-format rows). Mitigated by (a) the column comment documenting the new semantics, and (b) a one-line note added to `skills/claude-usage-db/SKILL.md` so the skill description matches reality.
- **Users want recovered text for new-format rows.** Deferred: not faithful, see "Non-goals". If a future need arises, the right place is a separate, explicitly-named convenience view or materialized column that documents its derivation. Not part of this change.

## Test infrastructure

The implementation must provision the test harness. Two layers, both CI-runnable via `cargo test`:

**Parser layer** — unit test in `crates/claude-code-transcripts/src/types.rs` (or a sibling `tests.rs` module) exercising `serde_json::from_str::<Entry>` on each of the four behavior-matrix line shapes and asserting the resulting `Entry::LastPrompt(LastPromptEntry)` field values. Uses existing `serde_json` dependency; no new dev-deps. Covers the original parse-error fix.

**Schema/appender layer** — integration test at `crates/claude-code-transcripts-ingest/tests/last_prompt_format.rs`. Adds a fixture file `crates/claude-code-transcripts-ingest/tests/fixtures/last_prompt_formats.jsonl` containing four `last-prompt` lines covering the behavior matrix, plus the minimal envelope needed by the ingest pipeline (a `transcripts` parent record if required). The test invokes the ingest pipeline programmatically (the simplest viable entry point: refactor or expose a callable function from `run.rs` that takes an input directory and an output `Path`; if `run::run` is not currently callable from a test, this refactor is part of the implementation work). Then opens the resulting DuckDB and asserts the rows. New dev-dependency: `tempfile` (workspace-add). No `assert_cmd` / external-process invocation.

If exposing `run::run` for test-callability adds significant churn, fall back to a smaller integration test that builds an in-memory DuckDB, runs the schema bootstrap SQL, calls the parser's row-builder directly, appends the four rows, and queries — bypassing the file-discovery layer. This still covers schema↔parser column ordering, which is the highest-risk wiring.

## Acceptance criteria

1. **Real-corpus smoke (manual, non-blocking):** `cct ingest ~/.claude/projects/` completes with exit code 0 on the maintainer's machine. Confirms the originally-reported bug is fixed against live data. Not a merge gate; included as a maintainer checklist item only.
2. **Parser-layer test passes (CI):** unit test in `crates/claude-code-transcripts/` parses each of the four fixture-line shapes and asserts the corresponding `LastPromptEntry` field values match the behavior matrix.
3. **Schema/appender-layer test passes (CI):** integration test in `crates/claude-code-transcripts-ingest/tests/last_prompt_format.rs` ingests the fixture file and asserts:
   - row 1 (old format): `(last_prompt, leaf_uuid) = ('hello world', NULL)`
   - row 2 (new format): `(last_prompt, leaf_uuid) = (NULL, 'u1')`
   - row 3 (hypothetical both): `(last_prompt, leaf_uuid) = ('inline', 'u2')`
   - row 4 (hypothetical neither): `(last_prompt, leaf_uuid) = (NULL, NULL)`
   - `SELECT COUNT(*) FROM last_prompt_entries` returns 4
4. **All-NULL observability (CI):** the integration test also asserts that ingesting row 4 increments the `unknown_variants["last-prompt:no-fields"]` counter (or equivalent observable signal) by exactly 1.
5. **Existing column-shape queries don't error (manual):** `SELECT entry_id, last_prompt, session_id FROM last_prompt_entries LIMIT 10` parses and returns rows on the user's corpus, producing NULL for `last_prompt` on new-format rows. Captured as part of criterion 1.
6. **Demo build (manual):** `scripts/demo/build.sh` runs to completion against a database built with the new schema; the resulting demo `last_prompt_entries` table contains the `leaf_uuid` column and pass-through values match the source.
7. **Skill survey at merge time (manual):** for every occurrence of `last_prompt` returned by `grep -rn 'last_prompt' skills/ docs/`, any embedded SQL example runs without DuckDB parser/binder error against the user's `transcripts.duckdb`. Documentation prose passes by inspection. This is a one-time check at merge; future skill authors honor the documented NULL-on-new-format contract.
8. **No new clippy / build warnings** introduced by the type or parser changes.

## Out of scope

- Backfilling old DuckDBs without re-ingest (no incremental ingest mode exists).
- Recovering typed prompt text for new-format rows (not faithfully recoverable).
- Convenience views for ancestor-walk lookup (separate change if needed).
- Altering the JSONL on-disk format.

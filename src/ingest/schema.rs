//! All DDL for the transcript ingest database.
//!
//! Schema decisions:
//! - Polymorphic / variable-shape fields stored as `JSON` (DuckDB JSON type).
//! - Native arrays/structs avoided to keep prepared-statement bindings to
//!   simple scalar types; nested data is serialised as JSON strings instead.
//!   Queryable via `json_extract`, `json_extract_string`, `->`, `->>`.

pub const SCHEMA_DDL: &str = r#"
-- ─────────────────────────────────────────────────────────────────────
-- Core
-- ─────────────────────────────────────────────────────────────────────

CREATE SEQUENCE IF NOT EXISTS entries_seq START 1;

CREATE TABLE IF NOT EXISTS transcripts (
    file_path           TEXT,
    session_id          TEXT,
    is_subagent         BOOLEAN,
    agent_id            TEXT,
    parent_session_id   TEXT,
    entry_count         INTEGER,
    first_timestamp     TIMESTAMP,
    last_timestamp      TIMESTAMP,
    mtime               TIMESTAMP,
    ingested_at         TIMESTAMP
);

CREATE TABLE IF NOT EXISTS entries (
    entry_id                BIGINT ,
    file_path               TEXT,
    line_no                 INTEGER,
    type                    TEXT,
    subtype                 TEXT,
    uuid                    TEXT,
    parent_uuid             TEXT,
    logical_parent_uuid     TEXT,
    is_sidechain            BOOLEAN,
    session_id              TEXT,
    timestamp               TIMESTAMP,
    user_type               TEXT,
    entrypoint              TEXT,
    cwd                     TEXT,
    version                 TEXT,
    git_branch              TEXT,
    slug                    TEXT,
    agent_id                TEXT,
    team_name               TEXT,
    agent_name              TEXT,
    agent_color             TEXT,
    prompt_id               TEXT,
    is_meta                 BOOLEAN,
    forked_from_uuid        TEXT,
    forked_from_session_id  TEXT,
    raw_json                TEXT
);

CREATE TABLE IF NOT EXISTS model_pricing (
    model                          TEXT ,
    input_per_mtok                 DOUBLE,
    output_per_mtok                DOUBLE,
    cache_creation_5m_per_mtok     DOUBLE,
    cache_creation_1h_per_mtok     DOUBLE,
    cache_read_per_mtok            DOUBLE,
    effective_date                 DATE
);

-- ─────────────────────────────────────────────────────────────────────
-- Rich variant tables
-- ─────────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS user_entries (
    entry_id                       BIGINT ,
    message_role                   TEXT,
    message_content_text           TEXT,
    message_has_blocks             BOOLEAN,
    tool_use_result                JSON,
    source_tool_assistant_uuid     TEXT,
    source_tool_use_id             TEXT,
    permission_mode                TEXT,
    origin                         JSON,
    is_compact_summary             BOOLEAN,
    is_visible_in_transcript_only  BOOLEAN,
    image_paste_ids                JSON,
    plan_content                   TEXT
);

CREATE TABLE IF NOT EXISTS assistant_entries (
    entry_id                        BIGINT ,
    message_id                      TEXT,
    role                            TEXT,
    model                           TEXT,
    container                       JSON,
    stop_reason                     TEXT,
    stop_sequence                   TEXT,
    stop_details                    JSON,
    context_management              JSON,
    request_id                      TEXT,
    is_api_error_message            BOOLEAN,
    error                           TEXT,
    tool_use_count                  INTEGER,
    cost_usd                        DOUBLE,
    input_tokens                    BIGINT,
    output_tokens                   BIGINT,
    cache_creation_input_tokens     BIGINT,
    cache_read_input_tokens         BIGINT,
    cache_creation_5m               BIGINT,
    cache_creation_1h               BIGINT,
    web_search_requests             BIGINT,
    web_fetch_requests              BIGINT,
    service_tier                    JSON,
    inference_geo                   JSON,
    iterations                      JSON,
    speed                           JSON,
    cost_per_tool_use DOUBLE GENERATED ALWAYS AS (cost_usd / NULLIF(tool_use_count, 0)) VIRTUAL
);

CREATE TABLE IF NOT EXISTS system_entries (
    entry_id                       BIGINT ,
    subtype                        TEXT,
    content                        TEXT,
    level                          TEXT,
    is_meta                        BOOLEAN,
    cause                          JSON,
    error                          JSON,
    retry_in_ms                    DOUBLE,
    retry_attempt                  INTEGER,
    max_retries                    INTEGER,
    hook_count                     INTEGER,
    hook_errors                    JSON,
    prevented_continuation         BOOLEAN,
    stop_reason                    TEXT,
    has_output                     BOOLEAN,
    tool_use_id                    TEXT,
    duration_ms                    DOUBLE,
    message_count                  INTEGER,
    url                            TEXT,
    upgrade_nudge                  TEXT,
    -- compact_metadata flattened (was STRUCT in spec; flattened for binding simplicity)
    compact_trigger                TEXT,
    compact_pre_tokens             BIGINT,
    compact_post_tokens            BIGINT,
    compact_duration_ms            BIGINT,
    compact_preserved_head_uuid    TEXT,
    compact_preserved_anchor_uuid  TEXT,
    compact_preserved_tail_uuid    TEXT,
    compact_pre_discovered_tools   JSON
);

CREATE TABLE IF NOT EXISTS attachment_entries (
    entry_id                    BIGINT ,
    attachment_type             TEXT,
    hook_name                   TEXT,
    tool_use_id                 TEXT,
    hook_event                  TEXT,
    hook_content                TEXT,
    hook_stdout                 TEXT,
    hook_stderr                 TEXT,
    hook_exit_code              INTEGER,
    hook_command                TEXT,
    hook_duration_ms            BIGINT,
    decision                    TEXT,
    filename                    TEXT,
    file_content_text           TEXT,
    file_content_metadata       JSON,
    display_path                TEXT,
    directory_path              TEXT,
    directory_content           TEXT,
    command_allowed_tools       JSON,
    plan_reminder_type          TEXT,
    plan_is_sub_agent           BOOLEAN,
    plan_file_path              TEXT,
    plan_exists                 BOOLEAN,
    skill_listing_content       TEXT,
    skill_listing_is_initial    BOOLEAN,
    skill_listing_count         INTEGER,
    skill_dir                   TEXT,
    skill_names                 JSON,
    invoked_skills              JSON,
    task_reminder_content       JSON,
    task_reminder_item_count    INTEGER,
    diagnostics_files           JSON,
    diagnostics_is_new          BOOLEAN,
    date_change_new_date        TEXT,
    deferred_added_names        JSON,
    deferred_added_lines        JSON,
    deferred_removed_names      JSON,
    mcp_added_names             JSON,
    mcp_added_blocks            JSON,
    mcp_removed_names           JSON,
    ultrathink_level            TEXT,
    queued_command_prompt       TEXT,
    queued_command_mode         TEXT
);

CREATE TABLE IF NOT EXISTS progress_entries (
    entry_id                BIGINT ,
    parent_tool_use_id      TEXT,
    tool_use_id             TEXT,
    data_type               TEXT,
    hook_event              TEXT,
    hook_name               TEXT,
    command                 TEXT,
    agent_id                TEXT,
    prompt                  TEXT,
    message                 JSON,
    query                   TEXT,
    result_count            INTEGER,
    elapsed_time_seconds    DOUBLE,
    full_output             TEXT,
    output                  TEXT,
    timeout_ms              BIGINT,
    total_lines             BIGINT,
    total_bytes             BIGINT,
    task_id                 TEXT,
    server_name             TEXT,
    status                  TEXT,
    tool_name               TEXT,
    elapsed_time_ms         DOUBLE,
    task_description        TEXT,
    task_type               TEXT
);

-- ─────────────────────────────────────────────────────────────────────
-- Child tables
-- ─────────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS user_content_blocks (
    entry_id                BIGINT,
    position                INTEGER,
    block_type              TEXT,
    text                    TEXT,
    tool_use_id             TEXT,
    tool_result_content     JSON,
    is_error                BOOLEAN,
    image_source            JSON,
    document_source         JSON,
    document_title          TEXT);

CREATE TABLE IF NOT EXISTS assistant_content_blocks (
    entry_id        BIGINT,
    position        INTEGER,
    block_type      TEXT,
    text            TEXT,
    thinking        TEXT,
    signature       TEXT,
    redacted_data   TEXT,
    tool_use_id     TEXT,
    tool_name       TEXT,
    tool_input      JSON,
    caller_type     TEXT);

CREATE TABLE IF NOT EXISTS assistant_usage_iterations (
    entry_id                       BIGINT,
    position                       INTEGER,
    iter_type                      TEXT,
    input_tokens                   BIGINT,
    output_tokens                  BIGINT,
    cache_read_input_tokens        BIGINT,
    cache_creation_input_tokens    BIGINT,
    cache_creation_5m              BIGINT,
    cache_creation_1h              BIGINT);

CREATE TABLE IF NOT EXISTS system_hook_infos (
    entry_id        BIGINT,
    position        INTEGER,
    command         TEXT,
    duration_ms     BIGINT);

CREATE TABLE IF NOT EXISTS attachment_diagnostics_files (
    entry_id        BIGINT,
    position        INTEGER,
    uri             TEXT,
    diagnostics     JSON);

CREATE TABLE IF NOT EXISTS attachment_invoked_skills (
    entry_id                BIGINT,
    position                INTEGER,
    skill_name              TEXT,
    invocation_metadata     JSON);

-- ─────────────────────────────────────────────────────────────────────
-- Metadata-only variant tables
-- ─────────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS permission_mode_entries (
    entry_id        BIGINT ,
    permission_mode TEXT,
    session_id      TEXT
);

CREATE TABLE IF NOT EXISTS last_prompt_entries (
    entry_id    BIGINT ,
    last_prompt TEXT,
    session_id  TEXT
);

CREATE TABLE IF NOT EXISTS ai_title_entries (
    entry_id    BIGINT ,
    ai_title    TEXT,
    session_id  TEXT
);

CREATE TABLE IF NOT EXISTS custom_title_entries (
    entry_id        BIGINT ,
    custom_title    TEXT,
    session_id      TEXT
);

CREATE TABLE IF NOT EXISTS agent_name_entries (
    entry_id    BIGINT ,
    agent_name  TEXT,
    session_id  TEXT
);

CREATE TABLE IF NOT EXISTS agent_color_entries (
    entry_id    BIGINT ,
    agent_color TEXT,
    session_id  TEXT
);

CREATE TABLE IF NOT EXISTS agent_setting_entries (
    entry_id        BIGINT ,
    agent_setting   TEXT,
    session_id      TEXT
);

CREATE TABLE IF NOT EXISTS tag_entries (
    entry_id    BIGINT ,
    tag         TEXT,
    session_id  TEXT
);

CREATE TABLE IF NOT EXISTS summary_entries (
    entry_id    BIGINT ,
    leaf_uuid   TEXT,
    summary     TEXT,
    session_id  TEXT
);

CREATE TABLE IF NOT EXISTS task_summary_entries (
    entry_id    BIGINT ,
    summary     TEXT,
    session_id  TEXT,
    timestamp   TIMESTAMP
);

CREATE TABLE IF NOT EXISTS pr_link_entries (
    entry_id        BIGINT ,
    session_id      TEXT,
    pr_number       INTEGER,
    pr_url          TEXT,
    pr_repository   TEXT,
    timestamp       TIMESTAMP
);

CREATE TABLE IF NOT EXISTS mode_entries (
    entry_id    BIGINT ,
    mode        TEXT,
    session_id  TEXT
);

CREATE TABLE IF NOT EXISTS worktree_state_entries (
    entry_id            BIGINT ,
    session_id          TEXT,
    worktree_session    JSON
);

CREATE TABLE IF NOT EXISTS content_replacement_entries (
    entry_id        BIGINT ,
    session_id      TEXT,
    replacements    JSON,
    agent_id        TEXT
);

CREATE TABLE IF NOT EXISTS file_history_snapshot_entries (
    entry_id                BIGINT ,
    message_id              TEXT,
    snapshot                JSON,
    is_snapshot_update      BOOLEAN
);

CREATE TABLE IF NOT EXISTS attribution_snapshot_entries (
    entry_id                                    BIGINT ,
    message_id                                  TEXT,
    surface                                     TEXT,
    file_states                                 JSON,
    prompt_count                                INTEGER,
    prompt_count_at_last_commit                 INTEGER,
    permission_prompt_count                     INTEGER,
    permission_prompt_count_at_last_commit      INTEGER,
    escape_count                                INTEGER,
    escape_count_at_last_commit                 INTEGER
);

CREATE TABLE IF NOT EXISTS queue_operation_entries (
    entry_id    BIGINT ,
    operation   TEXT,
    timestamp   TIMESTAMP,
    session_id  TEXT,
    content     TEXT
);

CREATE TABLE IF NOT EXISTS marble_origami_commit_entries (
    entry_id                BIGINT ,
    session_id              TEXT,
    collapse_id             TEXT,
    summary_uuid            TEXT,
    summary_content         TEXT,
    summary                 TEXT,
    first_archived_uuid     TEXT,
    last_archived_uuid      TEXT
);

CREATE TABLE IF NOT EXISTS marble_origami_snapshot_entries (
    entry_id            BIGINT ,
    session_id          TEXT,
    staged              JSON,
    armed               BOOLEAN,
    last_spawn_tokens   BIGINT
);

CREATE TABLE IF NOT EXISTS speculation_accept_entries (
    entry_id        BIGINT ,
    timestamp       TIMESTAMP,
    time_saved_ms   BIGINT
);
"#;

pub const TOOL_USES_VIEW_DDL: &str = r#"
CREATE OR REPLACE VIEW tool_uses AS
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
"#;

// PK uniqueness preserved via UNIQUE indexes built once post-ingest
// instead of per-row during append.
pub const INDEXES_DDL: &str = r#"
CREATE UNIQUE INDEX IF NOT EXISTS uq_transcripts_pk                         ON transcripts(file_path);
CREATE UNIQUE INDEX IF NOT EXISTS uq_entries_pk                             ON entries(entry_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_model_pricing_pk                       ON model_pricing(model);
CREATE UNIQUE INDEX IF NOT EXISTS uq_user_entries_pk                        ON user_entries(entry_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_assistant_entries_pk                   ON assistant_entries(entry_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_system_entries_pk                      ON system_entries(entry_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_attachment_entries_pk                  ON attachment_entries(entry_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_progress_entries_pk                    ON progress_entries(entry_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_user_content_blocks_pk                 ON user_content_blocks(entry_id, position);
CREATE UNIQUE INDEX IF NOT EXISTS uq_assistant_content_blocks_pk            ON assistant_content_blocks(entry_id, position);
CREATE UNIQUE INDEX IF NOT EXISTS uq_assistant_usage_iterations_pk          ON assistant_usage_iterations(entry_id, position);
CREATE UNIQUE INDEX IF NOT EXISTS uq_system_hook_infos_pk                   ON system_hook_infos(entry_id, position);
CREATE UNIQUE INDEX IF NOT EXISTS uq_attachment_diagnostics_files_pk        ON attachment_diagnostics_files(entry_id, position);
CREATE UNIQUE INDEX IF NOT EXISTS uq_attachment_invoked_skills_pk           ON attachment_invoked_skills(entry_id, position);
CREATE UNIQUE INDEX IF NOT EXISTS uq_permission_mode_entries_pk             ON permission_mode_entries(entry_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_last_prompt_entries_pk                 ON last_prompt_entries(entry_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_ai_title_entries_pk                    ON ai_title_entries(entry_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_custom_title_entries_pk                ON custom_title_entries(entry_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_agent_name_entries_pk                  ON agent_name_entries(entry_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_agent_color_entries_pk                 ON agent_color_entries(entry_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_agent_setting_entries_pk               ON agent_setting_entries(entry_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_tag_entries_pk                         ON tag_entries(entry_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_summary_entries_pk                     ON summary_entries(entry_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_task_summary_entries_pk                ON task_summary_entries(entry_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_pr_link_entries_pk                     ON pr_link_entries(entry_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_mode_entries_pk                        ON mode_entries(entry_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_worktree_state_entries_pk              ON worktree_state_entries(entry_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_content_replacement_entries_pk         ON content_replacement_entries(entry_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_file_history_snapshot_entries_pk       ON file_history_snapshot_entries(entry_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_attribution_snapshot_entries_pk        ON attribution_snapshot_entries(entry_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_queue_operation_entries_pk             ON queue_operation_entries(entry_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_marble_origami_commit_entries_pk       ON marble_origami_commit_entries(entry_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_marble_origami_snapshot_entries_pk     ON marble_origami_snapshot_entries(entry_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_speculation_accept_entries_pk          ON speculation_accept_entries(entry_id);
CREATE INDEX IF NOT EXISTS idx_entries_session_id    ON entries(session_id);
CREATE INDEX IF NOT EXISTS idx_entries_timestamp     ON entries(timestamp);
CREATE INDEX IF NOT EXISTS idx_entries_type          ON entries(type);
CREATE INDEX IF NOT EXISTS idx_entries_parent_uuid   ON entries(parent_uuid);
CREATE INDEX IF NOT EXISTS idx_entries_file_path     ON entries(file_path);
CREATE INDEX IF NOT EXISTS idx_assistant_model       ON assistant_entries(model);
CREATE INDEX IF NOT EXISTS idx_assistant_cost        ON assistant_entries(cost_usd);
CREATE INDEX IF NOT EXISTS idx_assistant_block_tool  ON assistant_content_blocks(tool_name);
CREATE INDEX IF NOT EXISTS idx_attachment_type       ON attachment_entries(attachment_type);
CREATE INDEX IF NOT EXISTS idx_transcripts_session   ON transcripts(session_id);
"#;

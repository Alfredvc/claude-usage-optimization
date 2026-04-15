//! HTTP server for the Claude Code transcript viewer.
//!
//! Reads from a DuckDB database built by `claude-code-transcripts-ingest ingest`.
//! Serves the embedded `web/index.html` and a JSON API backed by the DB.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Json, Response},
    routing::get,
    Router,
};
use duckdb::Connection;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::task::spawn_blocking;

use crate::cli::ServeArgs;

// ── State ─────────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct AppState {
    db_path: String,
    html: String,
}

fn open_db(db_path: &str) -> Result<Connection, String> {
    Connection::open(db_path).map_err(|e| format!("open {db_path}: {e}"))
}

// ── display_name ──────────────────────────────────────────────────────────────

fn home_dir() -> String {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default()
}

fn home_key() -> String {
    home_dir()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

fn find_real_path(base: &Path, suffix: &str) -> Option<PathBuf> {
    if suffix.is_empty() {
        return Some(base.to_path_buf());
    }
    if !suffix.starts_with('-') {
        return None;
    }
    let parts: Vec<&str> = suffix[1..].split('-').collect();
    for n in 1..=parts.len() {
        let name = parts[..n].join("-");
        let candidate = base.join(&name);
        let remaining = if n < parts.len() {
            format!("-{}", parts[n..].join("-"))
        } else {
            String::new()
        };
        if candidate.exists() {
            if remaining.is_empty() {
                return Some(candidate);
            }
            if let Some(r) = find_real_path(&candidate, &remaining) {
                return Some(r);
            }
        }
    }
    None
}

fn display_name(key: &str) -> String {
    let hk = home_key();
    if key.starts_with(&hk) {
        let suffix = &key[hk.len()..];
        let home = PathBuf::from(home_dir());
        if let Some(real) = find_real_path(&home, suffix) {
            if let Ok(rel) = real.strip_prefix(&home) {
                return format!("~/{}", rel.display());
            }
        }
        return format!("~{}", suffix.replace('-', "/"));
    }
    format!("/{}", key.replace('-', "/").trim_start_matches('/'))
}

// ── Text helpers ──────────────────────────────────────────────────────────────

fn extract_tool_result_text(json_str: &str) -> String {
    match serde_json::from_str::<Value>(json_str) {
        Ok(Value::String(s)) => s,
        Ok(Value::Array(arr)) => arr
            .iter()
            .filter_map(|item| {
                if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                    item.get("text").and_then(|t| t.as_str()).map(str::to_owned)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

fn extract_agent_id(text: &str) -> Option<String> {
    for line in text.lines() {
        if let Some(rest) = line.trim().strip_prefix("agentId:") {
            let id = rest.trim();
            if !id.is_empty() {
                return Some(id.to_owned());
            }
        }
    }
    None
}

fn summarize_input(name: &str, input: &Value) -> String {
    let short = |p: &str| -> String {
        let parts: Vec<&str> = p.split('/').collect();
        if parts.len() >= 2 {
            parts[parts.len() - 2..].join("/")
        } else {
            p.to_owned()
        }
    };

    match name {
        "Read" => {
            if let Some(fp) = input.get("file_path").and_then(|v| v.as_str()) {
                let suffix = input
                    .get("limit")
                    .and_then(|v| v.as_i64())
                    .map(|n| format!(", {n} lines"))
                    .unwrap_or_default();
                return format!("Read({}{})", short(fp), suffix);
            }
        }
        "Write" => {
            if let Some(fp) = input.get("file_path").and_then(|v| v.as_str()) {
                let lines = input
                    .get("content")
                    .and_then(|v| v.as_str())
                    .map(|c| c.chars().filter(|&c| c == '\n').count() + 1)
                    .unwrap_or(0);
                return format!("Write({}, {lines} lines)", short(fp));
            }
        }
        "Edit" | "MultiEdit" => {
            if let Some(fp) = input.get("file_path").and_then(|v| v.as_str()) {
                let lines = input
                    .get("old_string")
                    .and_then(|v| v.as_str())
                    .map(|s| s.chars().filter(|&c| c == '\n').count() + 1)
                    .unwrap_or(0);
                return format!("{name}({}, {lines} lines)", short(fp));
            }
        }
        _ => {}
    }

    let val = [
        "file_path",
        "pattern",
        "description",
        "command",
        "prompt",
        "query",
        "old_string",
        "skill",
        "subject",
        "path",
    ]
    .iter()
    .find_map(|k| {
        input
            .get(k)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
    });

    if let Some(v) = val {
        let s: String = v.chars().take(55).collect();
        let ell = if v.len() > 55 { "…" } else { "" };
        return format!("{}({}{})", name, s.replace('\n', " "), ell);
    }

    name.to_owned()
}

// ── Error helper ──────────────────────────────────────────────────────────────

fn err500(msg: impl std::fmt::Display) -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, msg.to_string()).into_response()
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn serve_index(State(state): State<AppState>) -> Html<String> {
    Html(state.html)
}

async fn api_projects(State(state): State<AppState>) -> Response {
    let db_path = state.db_path.clone();
    let result = spawn_blocking(move || -> Result<Value, String> {
        let conn = open_db(&db_path)?;
        let mut stmt = conn
            .prepare(
                "SELECT \
               regexp_extract(file_path, '.*/projects/([^/]+)/[^/]+\\.jsonl$', 1) AS project_key, \
               COUNT(*) AS session_count \
             FROM transcripts \
             WHERE NOT is_subagent \
               AND regexp_extract(file_path, '.*/projects/([^/]+)/[^/]+\\.jsonl$', 1) != '' \
             GROUP BY project_key \
             ORDER BY MAX(last_timestamp) DESC NULLS LAST",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|e| e.to_string())?;

        let mut out = Vec::new();
        for row in rows.filter_map(|r| r.ok()) {
            let (key, count) = row;
            if key.is_empty() {
                continue;
            }
            out.push(json!({
                "key":          key.clone(),
                "display":      display_name(&key),
                "sessionCount": count,
            }));
        }
        Ok(Value::Array(out))
    })
    .await;

    match result {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => err500(e),
        Err(e) => err500(e),
    }
}

#[derive(Deserialize)]
struct ProjectQ {
    project: Option<String>,
}

async fn api_sessions(State(state): State<AppState>, Query(q): Query<ProjectQ>) -> Response {
    let project = q.project.unwrap_or_default();
    if project.is_empty() {
        return Json(json!([])).into_response();
    }

    let db_path = state.db_path.clone();
    let result = spawn_blocking(move || -> Result<Value, String> {
        let conn = open_db(&db_path)?;
        let mut stmt = conn
            .prepare(
                "SELECT \
               t.session_id, \
               CAST(t.first_timestamp AS VARCHAR) AS started_at, \
               CAST(t.last_timestamp  AS VARCHAR) AS last_active, \
               ROUND(COALESCE(SUM(d.cost_usd), 0.0), 6) AS cost_usd, \
               EXISTS( \
                 SELECT 1 FROM transcripts t2 \
                 WHERE t2.parent_session_id = t.session_id \
               ) AS has_subagents \
             FROM transcripts t \
             LEFT JOIN entries e ON e.file_path = t.file_path \
             LEFT JOIN assistant_entries_deduped d \
                    ON d.entry_id = e.entry_id AND d.message_id IS NOT NULL \
             WHERE NOT t.is_subagent \
               AND regexp_extract(t.file_path, '.*/projects/([^/]+)/[^/]+\\.jsonl$', 1) = ? \
             GROUP BY t.session_id, t.first_timestamp, t.last_timestamp \
             ORDER BY t.last_timestamp DESC NULLS LAST",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([&project], |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, f64>(3)?,
                    row.get::<_, bool>(4)?,
                ))
            })
            .map_err(|e| e.to_string())?;

        let mut out = Vec::new();
        for row in rows.filter_map(|r| r.ok()) {
            let (id, started_at, last_active, cost_usd, has_subagents) = row;
            out.push(json!({
                "id":           id,
                "startedAt":    started_at,
                "lastActive":   last_active,
                "costUsd":      cost_usd,
                "hasSubagents": has_subagents,
            }));
        }
        Ok(Value::Array(out))
    })
    .await;

    match result {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => err500(e),
        Err(e) => err500(e),
    }
}

#[derive(Deserialize)]
struct TranscriptQ {
    #[allow(dead_code)]
    project: Option<String>,
    session: Option<String>,
}

async fn api_transcript(State(state): State<AppState>, Query(q): Query<TranscriptQ>) -> Response {
    let session = q.session.unwrap_or_default();
    if session.is_empty() {
        return (StatusCode::BAD_REQUEST, "session required").into_response();
    }
    let db_path = state.db_path.clone();
    let result = spawn_blocking(move || -> Result<Value, String> {
        let conn = open_db(&db_path)?;
        let fp = session_file_path(&conn, &session, false, None)?;
        build_timeline(&conn, &fp)
    })
    .await;

    match result {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => (StatusCode::NOT_FOUND, e).into_response(),
        Err(e) => err500(e),
    }
}

#[derive(Deserialize)]
struct SubagentQ {
    session: Option<String>,
    agent: Option<String>,
}

async fn api_subagent(State(state): State<AppState>, Query(q): Query<SubagentQ>) -> Response {
    let session = q.session.unwrap_or_default();
    let agent = q.agent.unwrap_or_default();
    if session.is_empty() || agent.is_empty() {
        return (StatusCode::BAD_REQUEST, "session and agent required").into_response();
    }
    let db_path = state.db_path.clone();
    let result = spawn_blocking(move || -> Result<Value, String> {
        let conn = open_db(&db_path)?;
        let fp = session_file_path(&conn, &session, true, Some(&agent))?;
        build_timeline(&conn, &fp)
    })
    .await;

    match result {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => (StatusCode::NOT_FOUND, e).into_response(),
        Err(e) => err500(e),
    }
}

// ── DB helpers ────────────────────────────────────────────────────────────────

fn session_file_path(
    conn: &Connection,
    session_id: &str,
    is_subagent: bool,
    agent_id: Option<&str>,
) -> Result<String, String> {
    if is_subagent {
        let agent = agent_id.unwrap_or("");
        let mut stmt = conn
            .prepare(
                "SELECT file_path FROM transcripts \
             WHERE parent_session_id = ? AND agent_id = ? AND is_subagent LIMIT 1",
            )
            .map_err(|e| e.to_string())?;
        stmt.query_row([session_id, agent], |row| row.get::<_, String>(0))
            .map_err(|_| format!("subagent not found: session={session_id} agent={agent}"))
    } else {
        let mut stmt = conn
            .prepare(
                "SELECT file_path FROM transcripts \
             WHERE session_id = ? AND NOT is_subagent LIMIT 1",
            )
            .map_err(|e| e.to_string())?;
        stmt.query_row([session_id], |row| row.get::<_, String>(0))
            .map_err(|_| format!("session not found: {session_id}"))
    }
}

// ── Timeline builder ──────────────────────────────────────────────────────────

const INJECTED: &[&str] = &[
    "<local-command-caveat>",
    "<command-name>",
    "<command-message>",
    "<task-notification>",
    "<local-command-stdout>",
    "<system-reminder>",
];

fn build_timeline(conn: &Connection, file_path: &str) -> Result<Value, String> {
    // ── entries ───────────────────────────────────────────────────────────────
    struct EntryRow {
        entry_id: i64,
        entry_type: String,
        timestamp: Option<String>,
        is_sidechain: bool,
        is_meta: bool,
    }
    let mut stmt = conn
        .prepare(
            "SELECT entry_id, type, CAST(timestamp AS VARCHAR), \
                COALESCE(is_sidechain, false), COALESCE(is_meta, false) \
         FROM entries WHERE file_path = ? ORDER BY entry_id",
        )
        .map_err(|e| e.to_string())?;
    let entry_rows: Vec<EntryRow> = stmt
        .query_map([file_path], |row| {
            Ok(EntryRow {
                entry_id: row.get(0)?,
                entry_type: row.get(1)?,
                timestamp: row.get(2)?,
                is_sidechain: row.get(3)?,
                is_meta: row.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    // ── user_entries metadata ─────────────────────────────────────────────────
    let mut user_compact: std::collections::HashSet<i64> = Default::default();
    let mut user_plain_text: HashMap<i64, String> = HashMap::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT ue.entry_id, COALESCE(ue.is_compact_summary, false), ue.message_content_text \
             FROM user_entries ue \
             JOIN entries e ON e.entry_id = ue.entry_id \
             WHERE e.file_path = ?",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([file_path], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, bool>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        for r in rows.filter_map(|r| r.ok()) {
            let (eid, compact, plain) = r;
            if compact {
                user_compact.insert(eid);
            }
            if let Some(t) = plain {
                user_plain_text.insert(eid, t);
            }
        }
    }

    // ── user text blocks (for block-content messages) ─────────────────────────
    let mut user_block_text: HashMap<i64, String> = HashMap::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT entry_id, text \
             FROM user_content_blocks \
             WHERE entry_id IN (SELECT entry_id FROM entries WHERE file_path = ?) \
               AND block_type = 'text' AND text IS NOT NULL \
             ORDER BY entry_id, position",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([file_path], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?;
        for r in rows.filter_map(|r| r.ok()) {
            let (eid, text) = r;
            user_block_text.entry(eid).or_default().push_str(&text);
        }
    }

    // ── tool results: tool_use_id → text ─────────────────────────────────────
    let mut tool_results: HashMap<String, String> = HashMap::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT tool_use_id, tool_result_content \
             FROM user_content_blocks \
             WHERE entry_id IN (SELECT entry_id FROM entries WHERE file_path = ?) \
               AND block_type = 'tool_result' AND tool_use_id IS NOT NULL",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([file_path], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            })
            .map_err(|e| e.to_string())?;
        for r in rows.filter_map(|r| r.ok()) {
            let (tu_id, content_json) = r;
            let text = content_json
                .as_deref()
                .map(extract_tool_result_text)
                .unwrap_or_default();
            tool_results.insert(tu_id, text);
        }
    }

    // ── deduped assistant entry IDs ───────────────────────────────────────────
    let mut deduped_ids: std::collections::HashSet<i64> = Default::default();
    {
        let mut stmt = conn
            .prepare(
                "SELECT aed.entry_id \
             FROM assistant_entries_deduped aed \
             JOIN entries e ON e.entry_id = aed.entry_id \
             WHERE e.file_path = ?",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([file_path], |row| row.get::<_, i64>(0))
            .map_err(|e| e.to_string())?;
        for r in rows.filter_map(|r| r.ok()) {
            deduped_ids.insert(r);
        }
    }

    // ── assistant entry data ──────────────────────────────────────────────────
    struct AsstData {
        model: String,
        cost_usd: Option<f64>,
        input: i64,
        output: i64,
        cache_read: i64,
        cache_write: i64,
    }
    let mut asst_data: HashMap<i64, AsstData> = HashMap::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT aed.entry_id, aed.model, aed.cost_usd, \
                    aed.input_tokens, aed.output_tokens, \
                    COALESCE(aed.cache_read_input_tokens, 0), \
                    COALESCE(aed.cache_creation_5m, 0) + COALESCE(aed.cache_creation_1h, 0) \
                      + CASE \
                          WHEN COALESCE(aed.cache_creation_5m, 0) + COALESCE(aed.cache_creation_1h, 0) > 0 \
                          THEN 0 \
                          ELSE COALESCE(aed.cache_creation_input_tokens, 0) \
                        END \
             FROM assistant_entries_deduped aed \
             JOIN entries e ON e.entry_id = aed.entry_id \
             WHERE e.file_path = ?",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([file_path], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<f64>>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        for r in rows.filter_map(|r| r.ok()) {
            let (eid, model, cost, inp, out, cr, cw) = r;
            asst_data.insert(
                eid,
                AsstData {
                    model,
                    cost_usd: cost,
                    input: inp,
                    output: out,
                    cache_read: cr,
                    cache_write: cw,
                },
            );
        }
    }

    // ── assistant content blocks ───────────────────────────────────────────────
    // Each streaming JSONL entry for a message carries exactly one content block.
    // The deduped view picks one entry_id per message_id, but the blocks are spread
    // across all sibling entries. Join through message_id to collect all of them,
    // grouping under the deduped entry_id.
    struct Block {
        block_type: String,
        text: Option<String>,
        tu_id: Option<String>,
        tu_name: Option<String>,
        tu_input: Option<Value>,
    }
    let mut asst_blocks: HashMap<i64, Vec<Block>> = HashMap::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT aed.entry_id AS dedup_eid, acb.block_type, acb.text, \
                        acb.tool_use_id, acb.tool_name, acb.tool_input \
                 FROM assistant_entries_deduped aed \
                 JOIN entries e_dedup ON e_dedup.entry_id = aed.entry_id \
                   AND e_dedup.file_path = ? \
                 JOIN assistant_entries ae_dedup ON ae_dedup.entry_id = aed.entry_id \
                 JOIN entries e_all ON e_all.file_path = ? \
                 JOIN assistant_entries ae_all ON ae_all.entry_id = e_all.entry_id \
                   AND (   (ae_dedup.message_id IS NOT NULL \
                            AND ae_all.message_id = ae_dedup.message_id) \
                        OR (ae_dedup.message_id IS NULL \
                            AND ae_all.entry_id = aed.entry_id)) \
                 JOIN assistant_content_blocks acb ON acb.entry_id = ae_all.entry_id \
                 ORDER BY aed.entry_id, ae_all.entry_id, acb.position",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([file_path, file_path], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        for r in rows.filter_map(|r| r.ok()) {
            let (dedup_eid, bt, text, tu_id, tu_name, tu_input_json) = r;
            let tu_input = tu_input_json
                .as_deref()
                .and_then(|j| serde_json::from_str(j).ok());
            asst_blocks.entry(dedup_eid).or_default().push(Block {
                block_type: bt,
                text,
                tu_id,
                tu_name,
                tu_input,
            });
        }
    }

    // ── assemble ──────────────────────────────────────────────────────────────
    let mut out: Vec<Value> = Vec::new();
    let mut api_num: i64 = 0;

    for e in &entry_rows {
        if e.is_sidechain || e.is_meta {
            continue;
        }

        match e.entry_type.as_str() {
            "user" => {
                if user_compact.contains(&e.entry_id) {
                    continue;
                }
                let text = user_plain_text
                    .get(&e.entry_id)
                    .or_else(|| user_block_text.get(&e.entry_id))
                    .cloned()
                    .unwrap_or_default();
                if text.is_empty() {
                    continue;
                }
                if INJECTED.iter().any(|p| text.starts_with(p)) {
                    continue;
                }
                out.push(json!({
                    "kind":      "user",
                    "timestamp": e.timestamp,
                    "text":      text,
                }));
            }
            "assistant" => {
                if !deduped_ids.contains(&e.entry_id) {
                    continue;
                }
                let Some(ad) = asst_data.get(&e.entry_id) else {
                    continue;
                };
                api_num += 1;

                let blocks = asst_blocks
                    .get(&e.entry_id)
                    .map(|v| v.as_slice())
                    .unwrap_or(&[]);
                let has_thinking = blocks
                    .iter()
                    .any(|b| b.block_type == "thinking" || b.block_type == "redacted_thinking");
                let texts: Vec<String> = blocks
                    .iter()
                    .filter(|b| b.block_type == "text")
                    .filter_map(|b| b.text.clone())
                    .filter(|t| !t.is_empty())
                    .collect();
                let tool_uses: Vec<Value> = blocks
                    .iter()
                    .filter(|b| b.block_type == "tool_use")
                    .filter_map(|b| {
                        let id = b.tu_id.as_ref()?;
                        let name = b.tu_name.as_ref()?;
                        let input = b.tu_input.clone().unwrap_or(json!({}));
                        let summary = summarize_input(name, &input);
                        let result = tool_results.get(id).cloned().unwrap_or_default();
                        let agent_id = extract_agent_id(&result);
                        Some(json!({
                            "id":       id,
                            "name":     name,
                            "summary":  summary,
                            "input":    input,
                            "result":   result,
                            "agent_id": agent_id,
                        }))
                    })
                    .collect();

                out.push(json!({
                    "kind":                        "assistant",
                    "num":                         api_num,
                    "timestamp":                   e.timestamp,
                    "model":                       ad.model,
                    "cost_usd":                    ad.cost_usd,
                    "input_tokens":                ad.input,
                    "output_tokens":               ad.output,
                    "cache_read_input_tokens":     ad.cache_read,
                    "cache_creation_input_tokens": ad.cache_write,
                    "has_thinking":                has_thinking,
                    "texts":                       texts,
                    "tool_uses":                   tool_uses,
                }));
            }
            _ => {}
        }
    }

    Ok(json!({ "entries": out }))
}

// ── Dashboard endpoints ───────────────────────────────────────────────────────

#[derive(Deserialize)]
struct DashboardQ {
    from: Option<String>,
    to: Option<String>,
    project: Option<String>,
}

fn time_bounds(q: &DashboardQ) -> (String, String) {
    let from = q
        .from
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or("1900-01-01")
        .to_owned();
    let to =
        q.to.as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or("2100-01-01")
            .to_owned();
    (from, to)
}

async fn api_dashboard_summary(
    State(state): State<AppState>,
    Query(q): Query<DashboardQ>,
) -> Response {
    let (from, to) = time_bounds(&q);
    let db_path = state.db_path.clone();
    let result = spawn_blocking(move || -> Result<Value, String> {
        let conn = open_db(&db_path)?;
        let mut stmt = conn
            .prepare(
                "SELECT \
                   ROUND(COALESCE(SUM(d.cost_usd), 0.0), 4) AS cost_usd, \
                   COUNT(DISTINCT CASE WHEN NOT t.is_subagent THEN t.session_id END) AS session_count, \
                   COUNT(DISTINCT CASE WHEN t.is_subagent THEN t.session_id END) AS subagent_count, \
                   COUNT(d.entry_id) AS api_call_count \
                 FROM entries e \
                 JOIN transcripts t ON t.file_path = e.file_path \
                 JOIN assistant_entries_deduped d ON d.entry_id = e.entry_id AND d.message_id IS NOT NULL \
                 WHERE CAST(e.timestamp AS TIMESTAMP) >= CAST(? AS TIMESTAMP) \
                   AND CAST(e.timestamp AS TIMESTAMP) < CAST(? AS TIMESTAMP)",
            )
            .map_err(|e| e.to_string())?;
        let (cost_usd, session_count, subagent_count, api_call_count) = stmt
            .query_row([&from, &to], |row| {
                Ok((
                    row.get::<_, f64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let denom = session_count.max(1) as f64;
        let avg = (cost_usd / denom * 1_000_000.0).round() / 1_000_000.0;
        Ok(json!({
            "cost_usd":              cost_usd,
            "session_count":         session_count,
            "subagent_count":        subagent_count,
            "api_call_count":        api_call_count,
            "avg_cost_per_session":  avg,
        }))
    })
    .await;

    match result {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => err500(e),
        Err(e) => err500(e),
    }
}

async fn api_dashboard_daily(
    State(state): State<AppState>,
    Query(q): Query<DashboardQ>,
) -> Response {
    let (from, to) = time_bounds(&q);
    let db_path = state.db_path.clone();
    let result = spawn_blocking(move || -> Result<Value, String> {
        let conn = open_db(&db_path)?;
        let mut stmt = conn
            .prepare(
                "SELECT \
                   CAST(e.timestamp AS DATE)::VARCHAR AS date, \
                   ROUND(SUM(CASE WHEN d.model ILIKE '%opus%' THEN d.cost_usd ELSE 0.0 END), 4) AS cost_opus, \
                   ROUND(SUM(CASE WHEN d.model NOT ILIKE '%opus%' AND d.model NOT ILIKE '%haiku%' THEN d.cost_usd ELSE 0.0 END), 4) AS cost_sonnet, \
                   ROUND(SUM(CASE WHEN d.model ILIKE '%haiku%' THEN d.cost_usd ELSE 0.0 END), 4) AS cost_haiku \
                 FROM entries e \
                 JOIN assistant_entries_deduped d ON d.entry_id = e.entry_id AND d.message_id IS NOT NULL \
                 WHERE CAST(e.timestamp AS TIMESTAMP) >= CAST(? AS TIMESTAMP) \
                   AND CAST(e.timestamp AS TIMESTAMP) < CAST(? AS TIMESTAMP) \
                 GROUP BY CAST(e.timestamp AS DATE) \
                 ORDER BY CAST(e.timestamp AS DATE) ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([&from, &to], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, f64>(1)?,
                    row.get::<_, f64>(2)?,
                    row.get::<_, f64>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows.filter_map(|r| r.ok()) {
            let (date, cost_opus, cost_sonnet, cost_haiku) = r;
            out.push(json!({
                "date":        date,
                "cost_opus":   cost_opus,
                "cost_sonnet": cost_sonnet,
                "cost_haiku":  cost_haiku,
            }));
        }
        Ok(Value::Array(out))
    })
    .await;

    match result {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => err500(e),
        Err(e) => err500(e),
    }
}

async fn api_dashboard_models(
    State(state): State<AppState>,
    Query(q): Query<DashboardQ>,
) -> Response {
    let (from, to) = time_bounds(&q);
    let db_path = state.db_path.clone();
    let result = spawn_blocking(move || -> Result<Value, String> {
        let conn = open_db(&db_path)?;
        let mut stmt = conn
            .prepare(
                "SELECT \
                   d.model, \
                   COUNT(DISTINCT e.session_id) AS sessions, \
                   COUNT(d.entry_id) AS api_calls, \
                   ROUND(SUM(d.cost_usd), 4) AS cost_usd, \
                   ROUND(100.0 * SUM(d.cost_usd) / NULLIF(SUM(SUM(d.cost_usd)) OVER (), 0.0), 2) AS pct_spend, \
                   ROUND(SUM(d.cost_usd) / NULLIF(COUNT(d.entry_id), 0), 6) AS avg_cost_per_turn \
                 FROM entries e \
                 JOIN assistant_entries_deduped d ON d.entry_id = e.entry_id AND d.message_id IS NOT NULL \
                 WHERE CAST(e.timestamp AS TIMESTAMP) >= CAST(? AS TIMESTAMP) \
                   AND CAST(e.timestamp AS TIMESTAMP) < CAST(? AS TIMESTAMP) \
                 GROUP BY d.model \
                 ORDER BY cost_usd DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([&from, &to], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<f64>>(3)?,
                    row.get::<_, Option<f64>>(4)?,
                    row.get::<_, Option<f64>>(5)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows.filter_map(|r| r.ok()) {
            let (model, sessions, api_calls, cost_usd, pct_spend, avg_cost_per_turn) = r;
            out.push(json!({
                "model":             model,
                "sessions":          sessions,
                "api_calls":         api_calls,
                "cost_usd":          cost_usd.unwrap_or(0.0),
                "pct_spend":         pct_spend.unwrap_or(0.0),
                "avg_cost_per_turn": avg_cost_per_turn.unwrap_or(0.0),
            }));
        }
        Ok(Value::Array(out))
    })
    .await;

    match result {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => err500(e),
        Err(e) => err500(e),
    }
}

async fn api_dashboard_cache(
    State(state): State<AppState>,
    Query(q): Query<DashboardQ>,
) -> Response {
    let (from, to) = time_bounds(&q);
    let db_path = state.db_path.clone();
    let result = spawn_blocking(move || -> Result<Value, String> {
        let conn = open_db(&db_path)?;

        // global cache stats
        let mut stmt = conn
            .prepare(
                "SELECT \
                   COALESCE(SUM(d.cache_read_input_tokens), 0) AS cache_read_tokens, \
                   COALESCE(SUM( \
                     COALESCE(d.cache_creation_5m, 0) + COALESCE(d.cache_creation_1h, 0) \
                     + CASE WHEN COALESCE(d.cache_creation_5m, 0) + COALESCE(d.cache_creation_1h, 0) > 0 \
                            THEN 0 ELSE COALESCE(d.cache_creation_input_tokens, 0) END \
                   ), 0) AS cache_create_tokens, \
                   COALESCE(SUM(d.input_tokens), 0) + COALESCE(SUM(d.cache_read_input_tokens), 0) \
                   + COALESCE(SUM( \
                     COALESCE(d.cache_creation_5m, 0) + COALESCE(d.cache_creation_1h, 0) \
                     + CASE WHEN COALESCE(d.cache_creation_5m, 0) + COALESCE(d.cache_creation_1h, 0) > 0 \
                            THEN 0 ELSE COALESCE(d.cache_creation_input_tokens, 0) END \
                   ), 0) \
                   + COALESCE(SUM(d.output_tokens), 0) AS total_tokens \
                 FROM entries e \
                 JOIN assistant_entries_deduped d ON d.entry_id = e.entry_id AND d.message_id IS NOT NULL \
                 WHERE CAST(e.timestamp AS TIMESTAMP) >= CAST(? AS TIMESTAMP) \
                   AND CAST(e.timestamp AS TIMESTAMP) < CAST(? AS TIMESTAMP)",
            )
            .map_err(|e| e.to_string())?;
        let (cache_read, cache_create, total) = stmt
            .query_row([&from, &to], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let denom = total.max(1) as f64;
        let hit_rate = cache_read as f64 / denom;
        let create_rate = cache_create as f64 / denom;

        // thrash turns
        let mut stmt2 = conn
            .prepare(
                "SELECT \
                   d.entry_id, \
                   t.session_id, \
                   regexp_extract(t.file_path, '.*/projects/([^/]+)/[^/]+\\.jsonl$', 1) AS project, \
                   ROUND(COALESCE(d.cost_usd, 0.0), 4) AS cost_usd, \
                   COALESCE(d.cache_creation_5m, 0) + COALESCE(d.cache_creation_1h, 0) \
                     + CASE WHEN COALESCE(d.cache_creation_5m, 0) + COALESCE(d.cache_creation_1h, 0) > 0 \
                            THEN 0 ELSE COALESCE(d.cache_creation_input_tokens, 0) END AS cc_tokens, \
                   COALESCE(d.output_tokens, 0) AS output_tokens \
                 FROM entries e \
                 JOIN transcripts t ON t.file_path = e.file_path \
                 JOIN assistant_entries_deduped d ON d.entry_id = e.entry_id AND d.message_id IS NOT NULL \
                 WHERE CAST(e.timestamp AS TIMESTAMP) >= CAST(? AS TIMESTAMP) \
                   AND CAST(e.timestamp AS TIMESTAMP) < CAST(? AS TIMESTAMP) \
                   AND COALESCE(d.output_tokens, 0) < 200 \
                   AND (COALESCE(d.cache_creation_5m, 0) + COALESCE(d.cache_creation_1h, 0) \
                        + CASE WHEN COALESCE(d.cache_creation_5m, 0) + COALESCE(d.cache_creation_1h, 0) > 0 \
                               THEN 0 ELSE COALESCE(d.cache_creation_input_tokens, 0) END) > 10000 \
                 ORDER BY cc_tokens DESC \
                 LIMIT 10",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt2
            .query_map([&from, &to], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, f64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let mut thrash = Vec::new();
        for r in rows.filter_map(|r| r.ok()) {
            let (entry_id, session_id, project, cost_usd, cc_tokens, output_tokens) = r;
            thrash.push(json!({
                "entry_id":      entry_id,
                "session_id":    session_id,
                "project":       project,
                "cost_usd":      cost_usd,
                "cc_tokens":     cc_tokens,
                "output_tokens": output_tokens,
            }));
        }

        Ok(json!({
            "hit_rate":            hit_rate,
            "create_rate":         create_rate,
            "cache_read_tokens":   cache_read,
            "cache_create_tokens": cache_create,
            "total_tokens":        total,
            "thrash_turns":        thrash,
        }))
    })
    .await;

    match result {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => err500(e),
        Err(e) => err500(e),
    }
}

async fn api_dashboard_agents(
    State(state): State<AppState>,
    Query(q): Query<DashboardQ>,
) -> Response {
    let (from, to) = time_bounds(&q);
    let db_path = state.db_path.clone();
    let result = spawn_blocking(move || -> Result<Value, String> {
        let conn = open_db(&db_path)?;

        // call counts
        let mut stmt1 = conn
            .prepare(
                "SELECT \
                   COUNT(*) FILTER (WHERE json_extract_string(acb.tool_input, '$.model') IS NOT NULL) AS explicit_calls, \
                   COUNT(*) FILTER (WHERE json_extract_string(acb.tool_input, '$.model') IS NULL) AS inherited_calls \
                 FROM assistant_content_blocks acb \
                 JOIN entries e ON e.entry_id = acb.entry_id \
                 WHERE acb.block_type = 'tool_use' AND acb.tool_name = 'Agent' \
                   AND CAST(e.timestamp AS TIMESTAMP) >= CAST(? AS TIMESTAMP) \
                   AND CAST(e.timestamp AS TIMESTAMP) < CAST(? AS TIMESTAMP)",
            )
            .map_err(|e| e.to_string())?;
        let (explicit_calls, inherited_calls) = stmt1
            .query_row([&from, &to], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|e| e.to_string())?;

        // total subagent cost
        let mut stmt2 = conn
            .prepare(
                "SELECT ROUND(COALESCE(SUM(d.cost_usd), 0.0), 4) \
                 FROM transcripts t \
                 JOIN entries e ON e.file_path = t.file_path \
                 JOIN assistant_entries_deduped d ON d.entry_id = e.entry_id AND d.message_id IS NOT NULL \
                 WHERE t.is_subagent \
                   AND CAST(e.timestamp AS TIMESTAMP) >= CAST(? AS TIMESTAMP) \
                   AND CAST(e.timestamp AS TIMESTAMP) < CAST(? AS TIMESTAMP)",
            )
            .map_err(|e| e.to_string())?;
        let total_subagent_cost: f64 = stmt2
            .query_row([&from, &to], |row| row.get::<_, f64>(0))
            .map_err(|e| e.to_string())?;

        let total_agent_calls = explicit_calls + inherited_calls;
        let total_agent_calls_denom = total_agent_calls.max(1) as f64;
        let inherited_cost_usd =
            total_subagent_cost * inherited_calls as f64 / total_agent_calls_denom;
        let inherited_cost_usd = (inherited_cost_usd * 10_000.0).round() / 10_000.0;

        // subtypes
        let mut stmt3 = conn
            .prepare(
                "SELECT \
                   COALESCE(json_extract_string(acb.tool_input, '$.subagent_type'), 'general-purpose') AS subtype, \
                   COUNT(*) AS count \
                 FROM assistant_content_blocks acb \
                 JOIN entries e ON e.entry_id = acb.entry_id \
                 WHERE acb.block_type = 'tool_use' AND acb.tool_name = 'Agent' \
                   AND CAST(e.timestamp AS TIMESTAMP) >= CAST(? AS TIMESTAMP) \
                   AND CAST(e.timestamp AS TIMESTAMP) < CAST(? AS TIMESTAMP) \
                 GROUP BY 1 \
                 ORDER BY count DESC \
                 LIMIT 15",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt3
            .query_map([&from, &to], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|e| e.to_string())?;
        let mut subtypes = Vec::new();
        for r in rows.filter_map(|r| r.ok()) {
            let (subtype, count) = r;
            let cost = total_subagent_cost * count as f64 / total_agent_calls_denom;
            let cost = (cost * 10_000.0).round() / 10_000.0;
            subtypes.push(json!({
                "subtype":  subtype,
                "count":    count,
                "cost_usd": cost,
            }));
        }

        Ok(json!({
            "explicit_calls":      explicit_calls,
            "inherited_calls":     inherited_calls,
            "inherited_cost_usd":  inherited_cost_usd,
            "subtypes":            subtypes,
        }))
    })
    .await;

    match result {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => err500(e),
        Err(e) => err500(e),
    }
}

async fn api_dashboard_top_sessions(
    State(state): State<AppState>,
    Query(q): Query<DashboardQ>,
) -> Response {
    let (from, to) = time_bounds(&q);
    let db_path = state.db_path.clone();
    let result = spawn_blocking(move || -> Result<Value, String> {
        let conn = open_db(&db_path)?;
        let mut stmt = conn
            .prepare(
                "SELECT \
                   t.session_id, \
                   regexp_extract(t.file_path, '.*/projects/([^/]+)/[^/]+\\.jsonl$', 1) AS project, \
                   CAST(t.first_timestamp AS VARCHAR) AS started_at, \
                   ROUND(COALESCE(SUM(d.cost_usd), 0.0), 4) AS cost_usd, \
                   COUNT(DISTINCT d.entry_id) AS turn_count, \
                   COALESCE(( \
                     SELECT COUNT(*) FROM user_content_blocks ucb2 \
                     JOIN entries e2 ON e2.entry_id = ucb2.entry_id AND e2.file_path = t.file_path \
                     WHERE ucb2.is_error = true \
                   ), 0) AS error_count, \
                   COALESCE(( \
                     SELECT COUNT(*) FROM transcripts t2 \
                     WHERE t2.parent_session_id = t.session_id \
                   ), 0) AS subagent_count \
                 FROM transcripts t \
                 JOIN entries e ON e.file_path = t.file_path \
                 JOIN assistant_entries_deduped d ON d.entry_id = e.entry_id AND d.message_id IS NOT NULL \
                 WHERE NOT t.is_subagent \
                   AND CAST(t.first_timestamp AS TIMESTAMP) >= CAST(? AS TIMESTAMP) \
                   AND CAST(t.first_timestamp AS TIMESTAMP) < CAST(? AS TIMESTAMP) \
                 GROUP BY t.session_id, t.file_path, t.first_timestamp \
                 ORDER BY cost_usd DESC \
                 LIMIT 15",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([&from, &to], |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, f64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows.filter_map(|r| r.ok()) {
            let (session_id, project, started_at, cost_usd, turn_count, error_count, subagent_count) = r;
            out.push(json!({
                "session_id":     session_id,
                "project":        project,
                "started_at":     started_at,
                "cost_usd":       cost_usd,
                "turn_count":     turn_count,
                "error_count":    error_count,
                "subagent_count": subagent_count,
            }));
        }
        Ok(Value::Array(out))
    })
    .await;

    match result {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => err500(e),
        Err(e) => err500(e),
    }
}

async fn api_dashboard_project_summary(
    State(state): State<AppState>,
    Query(q): Query<DashboardQ>,
) -> Response {
    let project = q.project.clone().unwrap_or_default();
    if project.is_empty() {
        return (StatusCode::BAD_REQUEST, "project required").into_response();
    }
    let (from, to) = time_bounds(&q);
    let db_path = state.db_path.clone();
    let result = spawn_blocking(move || -> Result<Value, String> {
        let conn = open_db(&db_path)?;
        let mut stmt = conn
            .prepare(
                "SELECT \
                   ROUND(COALESCE(SUM(d.cost_usd), 0.0), 4) AS cost_usd, \
                   COUNT(DISTINCT CASE WHEN NOT t.is_subagent THEN t.session_id END) AS session_count, \
                   COUNT(DISTINCT CASE WHEN t.is_subagent THEN t.session_id END) AS subagent_count, \
                   COUNT(d.entry_id) AS api_call_count \
                 FROM entries e \
                 JOIN transcripts t ON t.file_path = e.file_path \
                 JOIN assistant_entries_deduped d ON d.entry_id = e.entry_id AND d.message_id IS NOT NULL \
                 WHERE regexp_extract(e.file_path, '.*/projects/([^/]+)/[^/]+\\.jsonl$', 1) = ? \
                   AND CAST(e.timestamp AS TIMESTAMP) >= CAST(? AS TIMESTAMP) \
                   AND CAST(e.timestamp AS TIMESTAMP) < CAST(? AS TIMESTAMP)",
            )
            .map_err(|e| e.to_string())?;
        let (cost_usd, session_count, subagent_count, api_call_count) = stmt
            .query_row([&project, &from, &to], |row| {
                Ok((
                    row.get::<_, f64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let denom = session_count.max(1) as f64;
        let avg = (cost_usd / denom * 1_000_000.0).round() / 1_000_000.0;
        Ok(json!({
            "cost_usd":              cost_usd,
            "session_count":         session_count,
            "subagent_count":        subagent_count,
            "api_call_count":        api_call_count,
            "avg_cost_per_session":  avg,
        }))
    })
    .await;

    match result {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => err500(e),
        Err(e) => err500(e),
    }
}

async fn api_dashboard_session_distribution(
    State(state): State<AppState>,
    Query(q): Query<DashboardQ>,
) -> Response {
    let project = q.project.clone().unwrap_or_default();
    if project.is_empty() {
        return (StatusCode::BAD_REQUEST, "project required").into_response();
    }
    let (from, to) = time_bounds(&q);
    let db_path = state.db_path.clone();
    let result = spawn_blocking(move || -> Result<Value, String> {
        let conn = open_db(&db_path)?;
        let mut stmt = conn
            .prepare(
                "SELECT \
                   bucket, \
                   COUNT(*) AS session_count, \
                   ROUND(SUM(session_cost), 4) AS total_cost, \
                   ROUND(AVG(session_cost), 4) AS avg_cost, \
                   ROUND(MAX(session_cost), 4) AS max_cost \
                 FROM ( \
                   SELECT \
                     t.session_id, \
                     COALESCE(SUM(d.cost_usd), 0.0) AS session_cost, \
                     COUNT(DISTINCT d.entry_id) AS turn_count, \
                     CASE \
                       WHEN COUNT(DISTINCT d.entry_id) < 20 THEN '<20' \
                       WHEN COUNT(DISTINCT d.entry_id) < 100 THEN '20-100' \
                       WHEN COUNT(DISTINCT d.entry_id) < 500 THEN '100-500' \
                       WHEN COUNT(DISTINCT d.entry_id) < 2000 THEN '500-2k' \
                       ELSE '2k+' \
                     END AS bucket \
                   FROM transcripts t \
                   JOIN entries e ON e.file_path = t.file_path \
                   JOIN assistant_entries_deduped d ON d.entry_id = e.entry_id AND d.message_id IS NOT NULL \
                   WHERE NOT t.is_subagent \
                     AND regexp_extract(t.file_path, '.*/projects/([^/]+)/[^/]+\\.jsonl$', 1) = ? \
                     AND CAST(t.first_timestamp AS TIMESTAMP) >= CAST(? AS TIMESTAMP) \
                     AND CAST(t.first_timestamp AS TIMESTAMP) < CAST(? AS TIMESTAMP) \
                   GROUP BY t.session_id \
                 ) sub \
                 GROUP BY bucket \
                 ORDER BY CASE bucket \
                   WHEN '<20' THEN 1 \
                   WHEN '20-100' THEN 2 \
                   WHEN '100-500' THEN 3 \
                   WHEN '500-2k' THEN 4 \
                   ELSE 5 \
                 END",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([&project, &from, &to], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, f64>(2)?,
                    row.get::<_, f64>(3)?,
                    row.get::<_, f64>(4)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows.filter_map(|r| r.ok()) {
            let (bucket, session_count, total_cost, avg_cost, max_cost) = r;
            out.push(json!({
                "bucket":        bucket,
                "session_count": session_count,
                "total_cost":    total_cost,
                "avg_cost":      avg_cost,
                "max_cost":      max_cost,
            }));
        }
        Ok(Value::Array(out))
    })
    .await;

    match result {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => err500(e),
        Err(e) => err500(e),
    }
}

async fn api_dashboard_file_hotspots(
    State(state): State<AppState>,
    Query(q): Query<DashboardQ>,
) -> Response {
    let project = q.project.clone().unwrap_or_default();
    if project.is_empty() {
        return (StatusCode::BAD_REQUEST, "project required").into_response();
    }
    let (from, to) = time_bounds(&q);
    let db_path = state.db_path.clone();
    let result = spawn_blocking(move || -> Result<Value, String> {
        let conn = open_db(&db_path)?;
        let mut stmt = conn
            .prepare(
                "SELECT \
                   json_extract_string(acb.tool_input, '$.file_path') AS file_path, \
                   COUNT(DISTINCT e.session_id) AS distinct_sessions, \
                   COUNT(*) AS total_reads \
                 FROM assistant_content_blocks acb \
                 JOIN entries e ON e.entry_id = acb.entry_id \
                 JOIN transcripts t ON t.file_path = e.file_path \
                 WHERE acb.block_type = 'tool_use' AND acb.tool_name = 'Read' \
                   AND NOT t.is_subagent \
                   AND regexp_extract(t.file_path, '.*/projects/([^/]+)/[^/]+\\.jsonl$', 1) = ? \
                   AND CAST(e.timestamp AS TIMESTAMP) >= CAST(? AS TIMESTAMP) \
                   AND CAST(e.timestamp AS TIMESTAMP) < CAST(? AS TIMESTAMP) \
                   AND json_extract_string(acb.tool_input, '$.file_path') IS NOT NULL \
                   AND json_extract_string(acb.tool_input, '$.file_path') != '' \
                 GROUP BY 1 \
                 ORDER BY 2 DESC \
                 LIMIT 30",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([&project, &from, &to], |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows.filter_map(|r| r.ok()) {
            let (file_path, distinct_sessions, total_reads) = r;
            out.push(json!({
                "file_path":         file_path,
                "distinct_sessions": distinct_sessions,
                "total_reads":       total_reads,
            }));
        }
        Ok(Value::Array(out))
    })
    .await;

    match result {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => err500(e),
        Err(e) => err500(e),
    }
}

async fn api_dashboard_errors(
    State(state): State<AppState>,
    Query(q): Query<DashboardQ>,
) -> Response {
    let project = q.project.clone().unwrap_or_default();
    if project.is_empty() {
        return (StatusCode::BAD_REQUEST, "project required").into_response();
    }
    let (from, to) = time_bounds(&q);
    let db_path = state.db_path.clone();
    let result = spawn_blocking(move || -> Result<Value, String> {
        let conn = open_db(&db_path)?;

        // Query A: error types
        let mut stmt_a = conn
            .prepare(
                "SELECT \
                   CASE \
                     WHEN tool_result_content::TEXT ILIKE '%permission denied%' \
                       OR tool_result_content::TEXT ILIKE '%Operation not permitted%' THEN 'permission_denied' \
                     WHEN tool_result_content::TEXT ILIKE '%No such file%' \
                       OR tool_result_content::TEXT ILIKE '%not found%' \
                       OR tool_result_content::TEXT ILIKE '%does not exist%' THEN 'no_such_file' \
                     WHEN tool_result_content::TEXT ILIKE '%timeout%' \
                       OR tool_result_content::TEXT ILIKE '%timed out%' THEN 'timeout' \
                     WHEN tool_result_content::TEXT ILIKE '%tool_use_error%' \
                       OR tool_result_content::TEXT ILIKE '%ToolUseError%' THEN 'tool_use_error' \
                     ELSE 'other' \
                   END AS error_type, \
                   COUNT(*) AS count, \
                   COUNT(DISTINCT e.session_id) AS sessions_affected \
                 FROM user_content_blocks ucb \
                 JOIN entries e ON e.entry_id = ucb.entry_id \
                 JOIN transcripts t ON t.file_path = e.file_path \
                 WHERE ucb.is_error = true \
                   AND NOT t.is_subagent \
                   AND regexp_extract(t.file_path, '.*/projects/([^/]+)/[^/]+\\.jsonl$', 1) = ? \
                   AND CAST(e.timestamp AS TIMESTAMP) >= CAST(? AS TIMESTAMP) \
                   AND CAST(e.timestamp AS TIMESTAMP) < CAST(? AS TIMESTAMP) \
                 GROUP BY error_type \
                 ORDER BY count DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows_a = stmt_a
            .query_map([&project, &from, &to], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let mut types = Vec::new();
        for r in rows_a.filter_map(|r| r.ok()) {
            let (error_type, count, sessions_affected) = r;
            types.push(json!({
                "error_type":        error_type,
                "count":             count,
                "sessions_affected": sessions_affected,
            }));
        }

        // Query B1: session costs
        let mut stmt_b1 = conn
            .prepare(
                "SELECT \
                   t.session_id, \
                   COALESCE(SUM(d.cost_usd), 0.0) AS session_cost, \
                   COUNT(DISTINCT d.entry_id) AS turn_count \
                 FROM transcripts t \
                 JOIN entries e ON e.file_path = t.file_path \
                 JOIN assistant_entries_deduped d ON d.entry_id = e.entry_id AND d.message_id IS NOT NULL \
                 WHERE NOT t.is_subagent \
                   AND regexp_extract(t.file_path, '.*/projects/([^/]+)/[^/]+\\.jsonl$', 1) = ? \
                   AND CAST(t.first_timestamp AS TIMESTAMP) >= CAST(? AS TIMESTAMP) \
                   AND CAST(t.first_timestamp AS TIMESTAMP) < CAST(? AS TIMESTAMP) \
                 GROUP BY t.session_id",
            )
            .map_err(|e| e.to_string())?;
        let rows_b1 = stmt_b1
            .query_map([&project, &from, &to], |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, f64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let mut session_cost: HashMap<String, (f64, i64)> = HashMap::new();
        for r in rows_b1.filter_map(|r| r.ok()) {
            let (sid, cost, turns) = r;
            if let Some(s) = sid {
                session_cost.insert(s, (cost, turns));
            }
        }

        // Query B2: session error counts
        let mut stmt_b2 = conn
            .prepare(
                "SELECT \
                   t.session_id, \
                   COUNT(*) AS error_count \
                 FROM transcripts t \
                 JOIN entries e ON e.file_path = t.file_path \
                 JOIN user_content_blocks ucb ON ucb.entry_id = e.entry_id \
                 WHERE ucb.is_error = true \
                   AND NOT t.is_subagent \
                   AND regexp_extract(t.file_path, '.*/projects/([^/]+)/[^/]+\\.jsonl$', 1) = ? \
                   AND CAST(t.first_timestamp AS TIMESTAMP) >= CAST(? AS TIMESTAMP) \
                   AND CAST(t.first_timestamp AS TIMESTAMP) < CAST(? AS TIMESTAMP) \
                 GROUP BY t.session_id",
            )
            .map_err(|e| e.to_string())?;
        let rows_b2 = stmt_b2
            .query_map([&project, &from, &to], |row| {
                Ok((row.get::<_, Option<String>>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|e| e.to_string())?;
        let mut session_errors: HashMap<String, i64> = HashMap::new();
        for r in rows_b2.filter_map(|r| r.ok()) {
            let (sid, ec) = r;
            if let Some(s) = sid {
                session_errors.insert(s, ec);
            }
        }

        // Bucket and aggregate
        fn bucket_for(err: i64) -> &'static str {
            if err == 0 {
                "0 errors"
            } else if err < 10 {
                "1-9"
            } else if err < 50 {
                "10-49"
            } else {
                "50+"
            }
        }

        // bucket -> (sessions, total_cost, total_turns, total_errors)
        let mut buckets: HashMap<&'static str, (i64, f64, i64, i64)> = HashMap::new();
        for (sid, (cost, turns)) in &session_cost {
            let err = *session_errors.get(sid).unwrap_or(&0);
            let b = bucket_for(err);
            let entry = buckets.entry(b).or_insert((0, 0.0, 0, 0));
            entry.0 += 1;
            entry.1 += cost;
            entry.2 += turns;
            entry.3 += err;
        }

        let order = ["0 errors", "1-9", "10-49", "50+"];
        let mut by_bucket = Vec::new();
        for label in &order {
            if let Some((sessions, total_cost, total_turns, total_errors)) = buckets.get(*label) {
                let turns_denom = (*total_turns).max(1) as f64;
                let avg_cost_per_turn =
                    ((total_cost / turns_denom) * 1_000_000.0).round() / 1_000_000.0;
                let errors_per_turn =
                    ((*total_errors as f64 / turns_denom) * 1_000_000.0).round() / 1_000_000.0;
                by_bucket.push(json!({
                    "bucket":            label,
                    "sessions":          sessions,
                    "avg_cost_per_turn": avg_cost_per_turn,
                    "errors_per_turn":   errors_per_turn,
                }));
            }
        }

        Ok(json!({
            "types":     types,
            "by_bucket": by_bucket,
        }))
    })
    .await;

    match result {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => err500(e),
        Err(e) => err500(e),
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

pub async fn run(args: ServeArgs) {
    let db_path = args.db.to_string_lossy().into_owned();
    let port = args.port;

    let html = include_str!("../../../web/index.html").to_string();

    let state = AppState { db_path, html };

    let app = Router::new()
        .route("/", get(serve_index))
        .route("/api/projects", get(api_projects))
        .route("/api/sessions", get(api_sessions))
        .route("/api/transcript", get(api_transcript))
        .route("/api/subagent", get(api_subagent))
        .route("/api/dashboard/summary", get(api_dashboard_summary))
        .route("/api/dashboard/daily", get(api_dashboard_daily))
        .route("/api/dashboard/models", get(api_dashboard_models))
        .route("/api/dashboard/cache", get(api_dashboard_cache))
        .route("/api/dashboard/agents", get(api_dashboard_agents))
        .route(
            "/api/dashboard/top-sessions",
            get(api_dashboard_top_sessions),
        )
        .route(
            "/api/dashboard/project-summary",
            get(api_dashboard_project_summary),
        )
        .route(
            "/api/dashboard/session-distribution",
            get(api_dashboard_session_distribution),
        )
        .route(
            "/api/dashboard/file-hotspots",
            get(api_dashboard_file_hotspots),
        )
        .route("/api/dashboard/errors", get(api_dashboard_errors))
        .with_state(state);

    let addr = format!("127.0.0.1:{port}");
    println!("Claude Usage Visualizer → http://{addr}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| {
            eprintln!("bind {addr}: {e}");
            std::process::exit(1)
        });
    axum::serve(listener, app).await.unwrap_or_else(|e| {
        eprintln!("serve: {e}");
        std::process::exit(1)
    });
}

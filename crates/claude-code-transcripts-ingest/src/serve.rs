//! HTTP server for the Claude Code transcript viewer.
//!
//! Reads from a DuckDB database built by `claude-code-transcripts-ingest ingest`.
//! Serves `web/index.html` and a JSON API backed by the DB.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use axum::{
    Router,
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Json, Response},
    routing::get,
};
use duckdb::Connection;
use serde::Deserialize;
use serde_json::{Value, json};
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
    .find_map(|k| input.get(k).and_then(|v| v.as_str()).filter(|s| !s.is_empty()));

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
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))
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

async fn api_transcript(
    State(state): State<AppState>,
    Query(q): Query<TranscriptQ>,
) -> Response {
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
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                ))
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
                "SELECT entry_id, block_type, text, tool_use_id, tool_name, tool_input \
             FROM assistant_content_blocks \
             WHERE entry_id IN (SELECT entry_id FROM entries WHERE file_path = ?) \
             ORDER BY entry_id, position",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([file_path], |row| {
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
            let (eid, bt, text, tu_id, tu_name, tu_input_json) = r;
            let tu_input = tu_input_json
                .as_deref()
                .and_then(|j| serde_json::from_str(j).ok());
            asst_blocks.entry(eid).or_default().push(Block {
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

// ── Entry point ───────────────────────────────────────────────────────────────

pub async fn run(args: ServeArgs) {
    let db_path = args.db.to_string_lossy().into_owned();
    let port = args.port;

    let html = std::fs::read_to_string("web/index.html").unwrap_or_else(|_| {
        "<h1>web/index.html not found — run from project root</h1>".to_string()
    });

    let state = AppState { db_path, html };

    let app = Router::new()
        .route("/", get(serve_index))
        .route("/api/projects", get(api_projects))
        .route("/api/sessions", get(api_sessions))
        .route("/api/transcript", get(api_transcript))
        .route("/api/subagent", get(api_subagent))
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

// @ts-nocheck
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router-dom";
import {
  ActivityBreakdown,
  CostChart,
  SessionTotals,
  renderItem,
} from "../legacy/App";

export function TranscriptPage() {
  const { id = "" } = useParams<{ id: string }>();
  const [searchParams] = useSearchParams();
  const project = searchParams.get("project") ?? "";
  const entry = searchParams.get("entry") ?? "";
  const navigate = useNavigate();

  const [timeline, setTimeline] = useState<any[] | null>(null);
  const [agentCache, setAgentCache] = useState<Record<string, any[]>>({});
  const [loading, setLoading] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const reqSeq = useRef(0);

  const load = useCallback(async () => {
    if (!id || !project) return;
    const seq = ++reqSeq.current;
    setLoading(true);
    setErr(null);
    setTimeline(null);
    setAgentCache({});
    try {
      const r = await fetch(
        `/api/transcript?project=${encodeURIComponent(
          project,
        )}&session=${encodeURIComponent(id)}`,
      );
      if (!r.ok) throw new Error(`HTTP ${r.status}`);
      const data = await r.json();
      if (seq === reqSeq.current) setTimeline(data.entries || []);
    } catch (e: any) {
      if (seq === reqSeq.current) setErr(String(e?.message ?? e));
    } finally {
      if (seq === reqSeq.current) setLoading(false);
    }
  }, [id, project]);

  useEffect(() => {
    load();
  }, [load]);

  // Preload subagent entries in parallel
  useEffect(() => {
    if (!timeline || !id) return;
    const ids: string[] = [];
    for (const item of timeline) {
      if (item.kind === "assistant") {
        for (const tu of item.tool_uses || []) {
          if (tu.agent_id) ids.push(tu.agent_id);
        }
      }
    }
    if (ids.length === 0) return;
    for (const agentId of ids) {
      fetch(
        `/api/subagent?session=${encodeURIComponent(
          id,
        )}&agent=${encodeURIComponent(agentId)}`,
      )
        .then((r) => {
          if (!r.ok) throw new Error(`HTTP ${r.status}`);
          return r.json();
        })
        .then((data) =>
          setAgentCache((prev) => ({
            ...prev,
            [agentId]: data.entries || [],
          })),
        )
        .catch(() => {});
    }
  }, [timeline, id]);

  // Scroll to target entry after timeline loads
  useEffect(() => {
    if (!timeline || !entry) return;
    requestAnimationFrame(() => {
      const el = document.getElementById(`entry-${entry}`);
      if (!el) return;
      el.scrollIntoView({ behavior: "smooth", block: "center" });
      (el as HTMLElement).style.outline = "2px solid var(--accent)";
      setTimeout(() => {
        (el as HTMLElement).style.outline = "";
      }, 2000);
    });
  }, [timeline, entry]);

  const apiItems = useMemo(
    () =>
      (timeline || []).flatMap((item: any, idx: number) =>
        item.kind === "assistant" ? [{ ...item, timelineIdx: idx }] : [],
      ),
    [timeline],
  );
  const maxCost = useMemo(
    () =>
      Math.max(
        1e-9,
        apiItems.reduce((s: number, i: any) => {
          const sub = (i.tool_uses || []).reduce(
            (a: number, tu: any) => a + (tu.subagent_cost_usd || 0),
            0,
          );
          return s + (i.cost_usd || 0) + sub;
        }, 0),
      ),
    [apiItems],
  );

  return (
    <>
      <div className="header" style={{ borderTop: "1px solid var(--border)" }}>
        <button
          className="btn"
          onClick={() => navigate(-1)}
          style={{ background: "var(--surface2)", color: "var(--text)" }}
        >
          ← Back to sessions
        </button>
        <span className="sep">·</span>
        <span
          className="breadcrumb"
          style={{ fontFamily: "JetBrains Mono, monospace" }}
        >
          {project && (
            <>
              <span>{project.replace(/^-/, "").replace(/-/g, "/")}</span>
              <span style={{ margin: "0 6px" }}>/</span>
            </>
          )}
          <span>{id.slice(0, 8)}</span>
        </span>
        <button
          className="btn"
          onClick={load}
          disabled={loading || !id || !project}
          style={{ marginLeft: "auto" }}
        >
          {loading ? "Loading…" : "Reload"}
        </button>
        {err && <span className="err">{err}</span>}
      </div>

      {!project && (
        <div className="timeline">
          <div className="empty">
            Missing project parameter. Navigate from the session list.
          </div>
        </div>
      )}

      {timeline && <SessionTotals timeline={timeline} />}
      {timeline && <CostChart apiItems={apiItems} />}
      {timeline && <ActivityBreakdown timeline={timeline} />}

      <div className="timeline">
        {!timeline && !loading && project && (
          <div className="empty">No transcript loaded</div>
        )}
        {loading && <div className="empty">Loading transcript…</div>}
        {timeline &&
          timeline.map((item: any, idx: number) =>
            renderItem(item, idx, maxCost, id, agentCache),
          )}
      </div>

      {timeline && (
        <div className="legend">
          <span>Token legend:</span>
          {[
            ["var(--tok-input)", "input"],
            ["var(--tok-cr)", "cache read"],
            ["var(--tok-cw)", "cache write"],
            ["var(--tok-out)", "output"],
            ["var(--tok-subagent)", "subagent"],
          ].map(([c, l]) => (
            <span key={l} className="leg-item">
              <span className="leg-swatch" style={{ background: c }} /> {l}
            </span>
          ))}
          <span
            style={{
              marginLeft: 10,
              color: "var(--muted)",
              fontStyle: "italic",
            }}
          >
            tok bar: 100% = 1M tokens · cost bar: 100% = total session cost ·
            cost from DB
          </span>
        </div>
      )}
    </>
  );
}

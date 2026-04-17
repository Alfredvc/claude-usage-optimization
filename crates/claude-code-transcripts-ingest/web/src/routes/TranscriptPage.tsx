// @ts-nocheck
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router-dom";
import {
  ActivityBreakdown,
  CostChart,
  SessionTotals,
  renderItem,
} from "../legacy/App";
import { useLayoutContext } from "./Layout";
import {
  MultiSelectPicker,
  type PickerOption,
} from "../components/MultiSelectPicker";

type CostFilter = "all" | "10c" | "1";
type SubagentFilter = "any" | "yes" | "no";
type SortField = "time" | "cost";
type SortOrder = "asc" | "desc";

const COST_THRESHOLDS: Record<CostFilter, number> = {
  all: 0,
  "10c": 0.1,
  "1": 1.0,
};

const SORT_OPTIONS: Array<{ value: SortField; label: string }> = [
  { value: "time", label: "Time" },
  { value: "cost", label: "Cost" },
];

function shortModel(m: string): string {
  if (!m) return "unknown";
  return m.replace("claude-", "").replace(/-\d{8}$/, "");
}

function fmtCost(c: number): string {
  if (!c) return "$0.00";
  if (c >= 1) return `$${c.toFixed(2)}`;
  if (c >= 0.01) return `$${c.toFixed(3)}`;
  return `$${c.toFixed(4)}`;
}

function hasSubagent(item: any): boolean {
  return (item.tool_uses || []).some((tu: any) => tu.agent_id);
}

export function TranscriptPage() {
  const { id = "" } = useParams<{ id: string }>();
  const [searchParams] = useSearchParams();
  const project = searchParams.get("project") ?? "";
  const entry = searchParams.get("entry") ?? "";
  const navigate = useNavigate();
  const { setNavExtras } = useLayoutContext();

  const [timeline, setTimeline] = useState<any[] | null>(null);
  const [agentCache, setAgentCache] = useState<Record<string, any[]>>({});
  const [loading, setLoading] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const reqSeq = useRef(0);

  // Filters & sort (page-local).
  const [tools, setTools] = useState<string[]>([]);
  const [models, setModels] = useState<string[]>([]);
  const [subagents, setSubagents] = useState<SubagentFilter>("any");
  const [costFilter, setCostFilter] = useState<CostFilter>("all");
  const [rangeNums, setRangeNums] = useState<[number, number] | null>(null);
  const [sortField, setSortField] = useState<SortField>("time");
  const [sortOrder, setSortOrder] = useState<SortOrder>("asc");

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

  useEffect(() => {
    setTools([]);
    setModels([]);
    setSubagents("any");
    setCostFilter("all");
    setRangeNums(null);
    setSortField("time");
    setSortOrder("asc");
  }, [id, project]);

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

  const toolOptions: PickerOption[] = useMemo(() => {
    const freq = new Map<string, number>();
    for (const i of apiItems) {
      for (const tu of i.tool_uses || []) {
        freq.set(tu.name, (freq.get(tu.name) ?? 0) + 1);
      }
    }
    return [...freq.entries()]
      .sort((a, b) => b[1] - a[1])
      .map(([name, count]) => ({ value: name, label: name, hint: `×${count}` }));
  }, [apiItems]);

  const modelOptions: PickerOption[] = useMemo(() => {
    const freq = new Map<string, number>();
    for (const i of apiItems) {
      const m = shortModel(i.model || "");
      freq.set(m, (freq.get(m) ?? 0) + 1);
    }
    return [...freq.entries()]
      .sort((a, b) => b[1] - a[1])
      .map(([name, count]) => ({ value: name, label: name, hint: `×${count}` }));
  }, [apiItems]);

  const toolSet = useMemo(() => new Set(tools), [tools]);
  const modelSet = useMemo(() => new Set(models), [models]);

  const keptAssistant = useMemo(() => {
    const kept = new Set<number>();
    const costMin = COST_THRESHOLDS[costFilter];
    apiItems.forEach((item: any, i: number) => {
      const num = i + 1;
      if (rangeNums && (num < rangeNums[0] || num > rangeNums[1])) return;
      if (toolSet.size > 0) {
        const names = (item.tool_uses || []).map((tu: any) => tu.name);
        if (!names.some((t: string) => toolSet.has(t))) return;
      }
      if (modelSet.size > 0) {
        if (!modelSet.has(shortModel(item.model || ""))) return;
      }
      if (subagents === "yes" && !hasSubagent(item)) return;
      if (subagents === "no" && hasSubagent(item)) return;
      const cost = item.cost_usd || 0;
      if (cost < costMin) return;
      kept.add(i);
    });
    return kept;
  }, [apiItems, toolSet, modelSet, subagents, costFilter, rangeNums]);

  const hasAnyFilter =
    tools.length > 0 ||
    models.length > 0 ||
    subagents !== "any" ||
    costFilter !== "all" ||
    rangeNums != null;

  const isDefaultSort = sortField === "time" && sortOrder === "asc";

  const visibleItems = useMemo(() => {
    if (!timeline) return [];

    if (!hasAnyFilter && isDefaultSort) {
      return timeline.map((item: any, idx: number) => ({ item, idx }));
    }

    type Group = {
      parts: Array<{ item: any; idx: number }>;
      sortKey: { time: number; cost: number };
      assistantApiIdx: number | null;
    };
    const groups: Group[] = [];
    let pendingUser: { item: any; idx: number } | null = null;
    const apiByTimelineIdx = new Map<number, number>();
    apiItems.forEach((it: any, i: number) =>
      apiByTimelineIdx.set(it.timelineIdx, i),
    );

    timeline.forEach((item: any, idx: number) => {
      if (item.kind === "user") {
        pendingUser = { item, idx };
      } else if (item.kind === "assistant") {
        const apiIdx = apiByTimelineIdx.get(idx) ?? null;
        const t = item.timestamp ? Date.parse(item.timestamp) : 0;
        const c = item.cost_usd || 0;
        const parts: Array<{ item: any; idx: number }> = [];
        if (pendingUser) parts.push(pendingUser);
        parts.push({ item, idx });
        groups.push({
          parts,
          sortKey: { time: t, cost: c },
          assistantApiIdx: apiIdx,
        });
        pendingUser = null;
      } else if (item.kind === "compact") {
        pendingUser = null;
        const t = item.timestamp ? Date.parse(item.timestamp) : 0;
        groups.push({
          parts: [{ item, idx }],
          sortKey: { time: t, cost: 0 },
          assistantApiIdx: null,
        });
      }
    });

    const filtered = hasAnyFilter
      ? groups.filter((g) =>
          g.assistantApiIdx == null
            ? true
            : keptAssistant.has(g.assistantApiIdx),
        )
      : groups;

    const dir = sortOrder === "asc" ? 1 : -1;
    const sorted = [...filtered].sort(
      (a, b) => (a.sortKey[sortField] - b.sortKey[sortField]) * dir,
    );

    return sorted.flatMap((g) => g.parts);
  }, [
    timeline,
    apiItems,
    keptAssistant,
    hasAnyFilter,
    isDefaultSort,
    sortField,
    sortOrder,
  ]);

  const keptCount = keptAssistant.size;
  const totalCount = apiItems.length;
  const keptCost = useMemo(() => {
    let s = 0;
    apiItems.forEach((item: any, i: number) => {
      if (keptAssistant.has(i)) s += item.cost_usd || 0;
    });
    return s;
  }, [apiItems, keptAssistant]);
  const totalCost = useMemo(
    () => apiItems.reduce((s: number, i: any) => s + (i.cost_usd || 0), 0),
    [apiItems],
  );

  const clearAll = () => {
    setTools([]);
    setModels([]);
    setSubagents("any");
    setCostFilter("all");
    setRangeNums(null);
  };

  const onCostRangeChange = useCallback(
    (a: number | null, b: number | null) => {
      if (a == null || b == null) setRangeNums(null);
      else setRangeNums([a, b]);
    },
    [],
  );

  useEffect(() => {
    setNavExtras(
      <>
        <button
          className="nav-back"
          onClick={() => navigate(-1)}
          title="Back to sessions"
        >
          ← Sessions
        </button>
        <span className="nav-breadcrumb">
          {project && (
            <>
              <span>{project.replace(/^-/, "").replace(/-/g, "/")}</span>
              <span className="nav-crumb-sep">/</span>
            </>
          )}
          <span>{id.slice(0, 8)}</span>
        </span>
        <button
          className="nav-reload"
          onClick={load}
          disabled={loading || !id || !project}
          title="Reload transcript"
        >
          {loading ? "Loading…" : "Reload"}
        </button>
        {err && <span className="err">{err}</span>}
      </>,
    );
    return () => setNavExtras(null);
  }, [setNavExtras, navigate, project, id, load, loading, err]);

  return (
    <div className="sl-root">
      {!project && (
        <div className="timeline">
          <div className="empty">
            Missing project parameter. Navigate from the session list.
          </div>
        </div>
      )}

      {timeline && <SessionTotals timeline={timeline} />}
      {timeline && apiItems.length >= 2 && (
        <CostChart
          apiItems={apiItems}
          rangeNums={rangeNums}
          onRangeChange={onCostRangeChange}
        />
      )}
      {timeline && <ActivityBreakdown timeline={timeline} />}

      {timeline && apiItems.length > 0 && (
        <div className="sl-filterbar">
          <div className="sl-row">
            <label>Cost</label>
            <div className="sort-group">
              {(
                [
                  ["all", "all"],
                  ["10c", "≥ $0.10"],
                  ["1", "≥ $1"],
                ] as [CostFilter, string][]
              ).map(([v, label]) => (
                <button
                  key={v}
                  className={`sort-btn ${costFilter === v ? "active" : ""}`}
                  onClick={() => setCostFilter(v)}
                >
                  {label}
                </button>
              ))}
            </div>

            <label>Subagents</label>
            <div className="sort-group">
              {(["any", "yes", "no"] as SubagentFilter[]).map((v) => (
                <button
                  key={v}
                  className={`sort-btn ${subagents === v ? "active" : ""}`}
                  onClick={() => setSubagents(v)}
                >
                  {v}
                </button>
              ))}
            </div>

            {rangeNums && (
              <span className="sl-data-range">
                range: API #{rangeNums[0]}–#{rangeNums[1]}
              </span>
            )}
            {rangeNums && (
              <button
                className="sort-btn"
                onClick={() => setRangeNums(null)}
                title="Clear range"
              >
                reset
              </button>
            )}

            <span className="sl-spacer" />

            {hasAnyFilter && (
              <button className="btn sl-clear" onClick={clearAll}>
                Clear filters
              </button>
            )}
          </div>

          <div className="sl-row sl-row-pickers">
            <MultiSelectPicker
              label="Models"
              options={modelOptions}
              selected={models}
              onChange={setModels}
              pillClassName="model"
              placeholder="Search models…"
            />
            <MultiSelectPicker
              label="Tools"
              options={toolOptions}
              selected={tools}
              onChange={setTools}
              pillClassName="tool"
              placeholder="Search tools…"
            />
          </div>
        </div>
      )}

      {timeline && apiItems.length > 0 && (
        <div className="sl-summary">
          <div className="sl-summary-stats">
            <strong>{keptCount}</strong>
            <span>of {totalCount} turns</span>
            <span className="sep">·</span>
            <span className="v-cost">{fmtCost(keptCost)}</span>
            <span className="sep">·</span>
            <span>of {fmtCost(totalCost)}</span>
            {loading && (
              <>
                <span className="sep">·</span>
                <span>loading…</span>
              </>
            )}
          </div>
          <div className="sl-summary-sort">
            <label>Sort</label>
            <select
              value={sortField}
              onChange={(e) => setSortField(e.target.value as SortField)}
            >
              {SORT_OPTIONS.map((o) => (
                <option key={o.value} value={o.value}>
                  {o.label}
                </option>
              ))}
            </select>
            <div className="sort-group">
              {(["desc", "asc"] as SortOrder[]).map((o) => (
                <button
                  key={o}
                  className={`sort-btn ${sortOrder === o ? "active" : ""}`}
                  onClick={() => setSortOrder(o)}
                  title={o === "desc" ? "Descending" : "Ascending"}
                >
                  {o === "desc" ? "↓" : "↑"}
                </button>
              ))}
            </div>
          </div>
        </div>
      )}

      <div className="timeline">
        {!timeline && !loading && project && (
          <div className="empty">No transcript loaded</div>
        )}
        {loading && <div className="empty">Loading transcript…</div>}
        {timeline && visibleItems.length === 0 && hasAnyFilter && (
          <div className="empty">No turns match the current filters.</div>
        )}
        {timeline &&
          visibleItems.map(({ item, idx }) =>
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
    </div>
  );
}

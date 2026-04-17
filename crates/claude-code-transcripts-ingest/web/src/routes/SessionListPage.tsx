import { useEffect, useMemo, useState } from "react";
import { Link, useSearchParams } from "react-router-dom";
import {
  DEFAULT_FILTER,
  fetchProjects,
  fetchSessions,
  fetchSessionsMeta,
  filterToQuery,
  queryToFilter,
  type ProjectRow,
  type SessionRow,
  type SessionsFilter,
  type SessionsMeta,
  type SortField,
  type SortOrder,
  type SubagentFilter,
} from "../api/sessions";
import {
  MultiSelectPicker,
  type PickerOption,
} from "../components/MultiSelectPicker";
import "../components/MultiSelectPicker.css";
import { Timeline } from "../components/Timeline";
import "../components/Timeline.css";

const SORT_OPTIONS: Array<{ value: SortField; label: string }> = [
  { value: "last", label: "Last active" },
  { value: "started", label: "Started" },
  { value: "cost", label: "Cost" },
  { value: "tokens", label: "Tokens" },
];

function fmtCost(c: number): string {
  if (!c) return "$0.00";
  if (c >= 1) return `$${c.toFixed(2)}`;
  if (c >= 0.01) return `$${c.toFixed(3)}`;
  return `$${c.toFixed(4)}`;
}
function fmtTok(n: number): string {
  if (n >= 1e6) return `${(n / 1e6).toFixed(2)}M`;
  if (n >= 1000) return `${(n / 1000).toFixed(1)}K`;
  return String(n);
}
function fmtDate(ts: string | null): string {
  if (!ts) return "?";
  return new Date(ts).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function sortSessions(rows: SessionRow[], sort: SortField, order: SortOrder): SessionRow[] {
  const dir = order === "asc" ? 1 : -1;
  const key = (s: SessionRow): number => {
    switch (sort) {
      case "cost":
        return s.costUsd;
      case "tokens":
        return s.totalTokens;
      case "started":
        return s.startedAt ? Date.parse(s.startedAt) : 0;
      case "last":
      default:
        return s.lastActive ? Date.parse(s.lastActive) : 0;
    }
  };
  return [...rows].sort((a, b) => (key(a) - key(b)) * dir);
}

export function SessionListPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const filter = useMemo<SessionsFilter>(
    () => queryToFilter(searchParams),
    [searchParams],
  );

  const [projects, setProjects] = useState<ProjectRow[]>([]);
  const [meta, setMeta] = useState<SessionsMeta | null>(null);
  const [sessions, setSessions] = useState<SessionRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    fetchProjects().then(setProjects).catch(() => {});
    fetchSessionsMeta().then(setMeta).catch(() => {});
  }, []);

  // Refetch when server-side filters change. Time range is applied client-side.
  const serverKey = useMemo(
    () =>
      JSON.stringify({
        p: filter.projects,
        t: filter.tools,
        s: filter.subagents,
      }),
    [filter.projects, filter.tools, filter.subagents],
  );

  useEffect(() => {
    setLoading(true);
    setErr(null);
    fetchSessions(filter)
      .then((d) => setSessions(d))
      .catch((e) => setErr(String(e)))
      .finally(() => setLoading(false));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [serverKey]);

  const updateFilter = (next: SessionsFilter) => {
    setSearchParams(filterToQuery(next), { replace: false });
  };

  const setRange = (tStart: string | null, tEnd: string | null) => {
    const next = filterToQuery({ ...filter, tStart, tEnd });
    setSearchParams(next, { replace: true });
  };

  const clearAll = () => updateFilter(DEFAULT_FILTER);

  const hasAnyFilter =
    filter.projects.length > 0 ||
    filter.tools.length > 0 ||
    filter.subagents !== "any" ||
    filter.tStart !== null ||
    filter.tEnd !== null;

  const projectOptions: PickerOption[] = useMemo(
    () =>
      projects.map((p) => ({
        value: p.key,
        label: p.display,
        hint: `${p.sessionCount}`,
      })),
    [projects],
  );

  const toolOptions: PickerOption[] = useMemo(
    () => (meta?.tools ?? []).map((t) => ({ value: t, label: t })),
    [meta],
  );

  const projectLabels = useMemo(() => {
    const m: Record<string, string> = {};
    for (const p of projects) m[p.key] = p.display;
    return m;
  }, [projects]);

  const dataStart = meta?.earliest ?? null;
  const dataEnd = meta?.latest ?? null;

  // Apply time filter + sort client-side.
  const visibleSessions = useMemo(() => {
    const startMs = filter.tStart ? Date.parse(filter.tStart) : -Infinity;
    const endMs = filter.tEnd ? Date.parse(filter.tEnd) : Infinity;
    const within = sessions.filter((s) => {
      const t = s.lastActive ? Date.parse(s.lastActive) : NaN;
      if (isNaN(t)) return false;
      return t >= startMs && t <= endMs;
    });
    return sortSessions(within, filter.sort, filter.order);
  }, [sessions, filter.tStart, filter.tEnd, filter.sort, filter.order]);

  const totalCost = useMemo(
    () => visibleSessions.reduce((s, r) => s + r.costUsd, 0),
    [visibleSessions],
  );
  const totalTokens = useMemo(
    () => visibleSessions.reduce((s, r) => s + r.totalTokens, 0),
    [visibleSessions],
  );

  return (
    <div className="sl-root">
      <Timeline
        sessions={sessions}
        tStart={filter.tStart}
        tEnd={filter.tEnd}
        onRangeChange={setRange}
        projectLabels={projectLabels}
      />

      <div className="sl-filterbar">
        <div className="sl-row">
          <label>Range</label>
          <input
            type="datetime-local"
            value={isoToLocal(filter.tStart ?? dataStart)}
            onChange={(e) =>
              updateFilter({
                ...filter,
                tStart: localToIso(e.target.value),
              })
            }
            min={isoToLocal(dataStart)}
            max={isoToLocal(dataEnd)}
          />
          <span className="sl-arrow">→</span>
          <input
            type="datetime-local"
            value={isoToLocal(filter.tEnd ?? dataEnd)}
            onChange={(e) =>
              updateFilter({ ...filter, tEnd: localToIso(e.target.value) })
            }
            min={isoToLocal(dataStart)}
            max={isoToLocal(dataEnd)}
          />
          {(filter.tStart !== null || filter.tEnd !== null) && (
            <button
              className="sort-btn"
              onClick={() =>
                updateFilter({ ...filter, tStart: null, tEnd: null })
              }
              title="Reset to full range"
            >
              reset
            </button>
          )}
          <span className="sl-data-range">
            data: {fmtDate(dataStart)} — {fmtDate(dataEnd)}
          </span>

          <span className="sl-spacer" />

          <label>Subagents</label>
          <div className="sort-group">
            {(["any", "yes", "no"] as SubagentFilter[]).map((v) => (
              <button
                key={v}
                className={`sort-btn ${filter.subagents === v ? "active" : ""}`}
                onClick={() => updateFilter({ ...filter, subagents: v })}
              >
                {v}
              </button>
            ))}
          </div>

          {hasAnyFilter && (
            <button className="btn sl-clear" onClick={clearAll}>
              Clear filters
            </button>
          )}
        </div>

        <div className="sl-row sl-row-pickers">
          <MultiSelectPicker
            label="Projects"
            options={projectOptions}
            selected={filter.projects}
            onChange={(projects) => updateFilter({ ...filter, projects })}
            placeholder="Search projects…"
          />
          <MultiSelectPicker
            label="Tools"
            options={toolOptions}
            selected={filter.tools}
            onChange={(tools) => updateFilter({ ...filter, tools })}
            pillClassName="tool"
            placeholder="Search tools…"
          />
        </div>
      </div>

      <div className="sl-summary">
        <div className="sl-summary-stats">
          <strong>{visibleSessions.length}</strong>
          <span>sessions</span>
          <span className="sep">·</span>
          <span className="v-cost">{fmtCost(totalCost)}</span>
          <span className="sep">·</span>
          <span>{fmtTok(totalTokens)} tok</span>
          {(filter.tStart || filter.tEnd) && (
            <>
              <span className="sep">·</span>
              <span>
                {fmtDate(filter.tStart ?? dataStart)} — {fmtDate(filter.tEnd ?? dataEnd)}
              </span>
            </>
          )}
          {loading && (
            <>
              <span className="sep">·</span>
              <span>loading…</span>
            </>
          )}
          {err && <span className="err">{err}</span>}
        </div>
        <div className="sl-summary-sort">
          <label>Sort</label>
          <select
            value={filter.sort}
            onChange={(e) =>
              updateFilter({ ...filter, sort: e.target.value as SortField })
            }
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
                className={`sort-btn ${filter.order === o ? "active" : ""}`}
                onClick={() => updateFilter({ ...filter, order: o })}
                title={o === "desc" ? "Descending" : "Ascending"}
              >
                {o === "desc" ? "↓" : "↑"}
              </button>
            ))}
          </div>
        </div>
      </div>

      <div className="sl-list">
        {visibleSessions.map((s) => (
          <SessionCard key={s.id} session={s} />
        ))}
      </div>
    </div>
  );
}

function SessionCard({ session }: { session: SessionRow }) {
  const qs = new URLSearchParams();
  if (session.project) qs.set("project", session.project);
  const to = `/transcripts/${encodeURIComponent(session.id)}?${qs.toString()}`;
  return (
    <Link to={to} className="sl-card">
      <div className="sl-card-head">
        <span className="sl-card-id">{session.id.slice(0, 8)}</span>
        <span className="sl-card-project" title={session.project}>
          {session.project.replace(/^-/, "").replace(/-/g, "/")}
        </span>
        {session.hasSubagents && (
          <span className="sl-tag agent">subagents</span>
        )}
        <span className="sl-card-cost">{fmtCost(session.costUsd)}</span>
      </div>
      <div className="sl-card-meta">
        <span>started {fmtDate(session.startedAt)}</span>
        <span>·</span>
        <span>last {fmtDate(session.lastActive)}</span>
        <span>·</span>
        <span>{fmtTok(session.totalTokens)} tok</span>
      </div>
      {session.tools.length > 0 && (
        <div className="sl-card-tools">
          {session.tools.slice(0, 10).map((t) => (
            <span key={t} className="sl-pill sm tool">
              {t}
            </span>
          ))}
          {session.tools.length > 10 && (
            <span className="sl-pill sm">+{session.tools.length - 10}</span>
          )}
        </div>
      )}
    </Link>
  );
}

function isoToLocal(iso: string | null): string {
  if (!iso) return "";
  const d = new Date(iso);
  if (isNaN(d.getTime())) return "";
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(
    d.getDate(),
  )}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}
function localToIso(local: string): string | null {
  if (!local) return null;
  const d = new Date(local);
  if (isNaN(d.getTime())) return null;
  return d.toISOString();
}

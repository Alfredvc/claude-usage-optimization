import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Bar,
  BarChart,
  CartesianGrid,
  Legend,
  ReferenceArea,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import type { SessionRow } from "../api/sessions";

const PALETTE = [
  "#f59e0b",
  "#3b82f6",
  "#22c55e",
  "#a78bfa",
  "#f472b6",
  "#06b6d4",
  "#eab308",
  "#ef4444",
  "#14b8a6",
  "#c084fc",
];
const OTHER_COLOR = "#6e7681";
const TOP_N = 8;
const DAY_MS = 86400000;

interface TimelineProps {
  sessions: SessionRow[];
  tStart: string | null;
  tEnd: string | null;
  onRangeChange: (t0: string | null, t1: string | null) => void;
  projectLabels?: Record<string, string>;
  height?: number;
}

interface Row {
  date: string;
  label: string;
  [project: string]: string | number;
}

function dayKey(iso: string): string {
  return iso.slice(0, 10);
}

function startOfDayIso(day: string): string {
  return new Date(day + "T00:00:00Z").toISOString();
}

function endOfDayIso(day: string): string {
  return new Date(day + "T23:59:59.999Z").toISOString();
}

function displayProject(key: string, labels?: Record<string, string>): string {
  if (key === "__other") return "other";
  return labels?.[key] ?? key.replace(/^-/, "").replace(/-/g, "/");
}

function fmtLabel(day: string): string {
  try {
    return new Date(day + "T12:00:00Z").toLocaleDateString(undefined, {
      month: "short",
      day: "numeric",
    });
  } catch {
    return day;
  }
}

export function Timeline({
  sessions,
  tStart,
  tEnd,
  onRangeChange,
  projectLabels,
  height = 220,
}: TimelineProps) {
  const { chartData, topProjects, colors } = useMemo(() => {
    const byDay = new Map<string, Map<string, number>>();
    const totals = new Map<string, number>();
    for (const s of sessions) {
      if (!s.lastActive) continue;
      const day = dayKey(s.lastActive);
      let inner = byDay.get(day);
      if (!inner) {
        inner = new Map();
        byDay.set(day, inner);
      }
      inner.set(s.project, (inner.get(s.project) ?? 0) + s.costUsd);
      totals.set(s.project, (totals.get(s.project) ?? 0) + s.costUsd);
    }
    const ranked = [...totals.entries()]
      .sort((a, b) => b[1] - a[1])
      .map((e) => e[0]);
    const top = ranked.slice(0, TOP_N);
    const topSet = new Set(top);

    const sortedDays = [...byDay.keys()].sort();
    if (sortedDays.length === 0) {
      return { chartData: [] as Row[], topProjects: [] as string[], colors: {} as Record<string, string> };
    }
    const startMs = Date.parse(sortedDays[0] + "T00:00:00Z");
    const endMs = Date.parse(sortedDays[sortedDays.length - 1] + "T00:00:00Z");

    const rows: Row[] = [];
    for (let t = startMs; t <= endMs; t += DAY_MS) {
      const d = new Date(t).toISOString().slice(0, 10);
      const inner = byDay.get(d);
      const row: Row = { date: d, label: fmtLabel(d) };
      let other = 0;
      if (inner) {
        for (const [proj, cost] of inner) {
          if (topSet.has(proj)) row[proj] = ((row[proj] as number) ?? 0) + cost;
          else other += cost;
        }
      }
      if (other > 0) row.__other = other;
      rows.push(row);
    }

    const cols: Record<string, string> = {};
    top.forEach((p, i) => {
      cols[p] = PALETTE[i % PALETTE.length];
    });
    cols.__other = OTHER_COLOR;
    return { chartData: rows, topProjects: top, colors: cols };
  }, [sessions]);

  // Drag-to-select range: capture active X label on mousedown/move/up, then
  // commit the range on release. `dragStart === null` means no drag in flight.
  const [dragStart, setDragStart] = useState<string | null>(null);
  const [dragEnd, setDragEnd] = useState<string | null>(null);
  const dragRef = useRef<{ start: string | null; end: string | null }>({
    start: null,
    end: null,
  });
  dragRef.current = { start: dragStart, end: dragEnd };

  const commitDrag = useCallback(() => {
    const { start, end } = dragRef.current;
    setDragStart(null);
    setDragEnd(null);
    if (!start) return;
    const lo = chartData.findIndex((r) => r.label === start);
    const hi = chartData.findIndex((r) => r.label === (end ?? start));
    if (lo < 0 || hi < 0) return;
    const a = Math.min(lo, hi);
    const b = Math.max(lo, hi);
    const fullRange = a === 0 && b === chartData.length - 1;
    if (fullRange) {
      onRangeChange(null, null);
      return;
    }
    onRangeChange(
      startOfDayIso(chartData[a].date),
      endOfDayIso(chartData[b].date),
    );
  }, [chartData, onRangeChange]);

  // Safety: if user releases mouse outside the chart (or off the page), still
  // commit the pending drag.
  useEffect(() => {
    if (!dragStart) return;
    const onUp = () => commitDrag();
    document.addEventListener("mouseup", onUp);
    return () => document.removeEventListener("mouseup", onUp);
  }, [dragStart, commitDrag]);

  const onChartMouseDown = useCallback(
    (e: { activeLabel?: string | number } | null) => {
      if (!e || e.activeLabel == null) return;
      const l = String(e.activeLabel);
      setDragStart(l);
      setDragEnd(l);
    },
    [],
  );

  const onChartMouseMove = useCallback(
    (e: { activeLabel?: string | number } | null) => {
      if (!dragStart || !e || e.activeLabel == null) return;
      setDragEnd(String(e.activeLabel));
    },
    [dragStart],
  );

  // Existing filter range rendered as a persistent overlay when not dragging.
  const filterOverlay = useMemo(() => {
    if (dragStart) return null;
    if (!tStart && !tEnd) return null;
    if (chartData.length === 0) return null;
    const sDay = tStart ? dayKey(tStart) : chartData[0].date;
    const eDay = tEnd ? dayKey(tEnd) : chartData[chartData.length - 1].date;
    let a = chartData.findIndex((r) => r.date >= sDay);
    if (a < 0) a = chartData.length - 1;
    let b = -1;
    for (let i = chartData.length - 1; i >= 0; i--) {
      if (chartData[i].date <= eDay) {
        b = i;
        break;
      }
    }
    if (b < 0) b = 0;
    if (a > b) [a, b] = [b, a];
    return { left: chartData[a].label, right: chartData[b].label };
  }, [chartData, tStart, tEnd, dragStart]);

  if (chartData.length === 0) {
    return <div className="tl-empty">No activity</div>;
  }

  const allKeys = [...topProjects, "__other"];

  return (
    <div
      className="tl-root"
      style={{ height, userSelect: dragStart ? "none" : undefined }}
    >
      <ResponsiveContainer width="100%" height="100%">
        <BarChart
          data={chartData}
          margin={{ top: 8, right: 12, left: 0, bottom: 0 }}
          onMouseDown={onChartMouseDown}
          onMouseMove={onChartMouseMove}
          onMouseUp={commitDrag}
          style={{ cursor: "crosshair" }}
        >
          <CartesianGrid strokeDasharray="3 3" stroke="#21262d" />
          <XAxis
            dataKey="label"
            stroke="#8b949e"
            tick={{ fontSize: 10 }}
            interval="preserveStartEnd"
          />
          <YAxis
            stroke="#8b949e"
            tick={{ fontSize: 10 }}
            tickFormatter={(v: number) => `$${v}`}
          />
          <Tooltip
            contentStyle={{
              background: "#1c2128",
              border: "1px solid #30363d",
              fontSize: 11,
            }}
            formatter={(v: number, n: string) => [
              `$${Number(v).toFixed(2)}`,
              displayProject(n, projectLabels),
            ]}
            labelFormatter={(l) => String(l)}
            isAnimationActive={false}
          />
          <Legend
            wrapperStyle={{ fontSize: 10 }}
            formatter={(v: string) => displayProject(v, projectLabels)}
          />
          {allKeys.map((k) => (
            <Bar
              key={k}
              dataKey={k}
              stackId="a"
              fill={colors[k] ?? OTHER_COLOR}
              isAnimationActive={false}
            />
          ))}
          {dragStart && dragEnd && (
            <ReferenceArea
              x1={dragStart}
              x2={dragEnd}
              strokeOpacity={0}
              fill="#58a6ff"
              fillOpacity={0.18}
            />
          )}
          {!dragStart && filterOverlay && (
            <ReferenceArea
              x1={filterOverlay.left}
              x2={filterOverlay.right}
              strokeOpacity={0}
              fill="#58a6ff"
              fillOpacity={0.08}
            />
          )}
        </BarChart>
      </ResponsiveContainer>
    </div>
  );
}

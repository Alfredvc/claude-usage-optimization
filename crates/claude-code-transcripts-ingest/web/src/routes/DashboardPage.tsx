import { useCallback, useEffect, useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { DashboardView } from "../legacy/App";
import { fetchProjects, type ProjectRow } from "../api/sessions";

const RANGES = ["7d", "30d", "90d", "all"] as const;
type Range = (typeof RANGES)[number];

export function DashboardPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const rangeParam = searchParams.get("range");
  const timeRange: Range =
    rangeParam && (RANGES as readonly string[]).includes(rangeParam)
      ? (rangeParam as Range)
      : "30d";

  const setTimeRange = (r: Range) => {
    const p = new URLSearchParams(searchParams);
    if (r === "30d") p.delete("range");
    else p.set("range", r);
    setSearchParams(p, { replace: true });
  };

  const [projects, setProjects] = useState<ProjectRow[]>([]);
  useEffect(() => {
    fetchProjects().then(setProjects).catch(() => {});
  }, []);

  const navigate = useNavigate();
  const navigateToSession = useCallback(
    (project: string, sessionId: string, entryId?: string | number) => {
      const qs = new URLSearchParams();
      if (project) qs.set("project", project);
      if (entryId != null) qs.set("entry", String(entryId));
      navigate(`/transcripts/${encodeURIComponent(sessionId)}?${qs}`);
    },
    [navigate],
  );

  return (
    <>
      <div className="header" style={{ borderTop: "1px solid var(--border)" }}>
        <span className="sep">Range</span>
        <div className="sort-group">
          {RANGES.map((r) => (
            <button
              key={r}
              className={`sort-btn ${timeRange === r ? "active" : ""}`}
              onClick={() => setTimeRange(r)}
            >
              {r === "all" ? "All" : r}
            </button>
          ))}
        </div>
      </div>
      <DashboardView
        timeRange={timeRange}
        projects={projects}
        navigateToSession={navigateToSession}
      />
    </>
  );
}

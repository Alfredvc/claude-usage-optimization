export type SortField = "cost" | "tokens" | "started" | "last";
export type SortOrder = "asc" | "desc";
export type SubagentFilter = "any" | "yes" | "no";

export interface SessionRow {
  id: string;
  project: string;
  startedAt: string | null;
  lastActive: string | null;
  costUsd: number;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheCreationTokens: number;
  totalTokens: number;
  hasSubagents: boolean;
  tools: string[];
}

export interface SessionsFilter {
  projects: string[];
  tools: string[];
  subagents: SubagentFilter;
  tStart: string | null;
  tEnd: string | null;
  sort: SortField;
  order: SortOrder;
}

export const DEFAULT_FILTER: SessionsFilter = {
  projects: [],
  tools: [],
  subagents: "any",
  tStart: null,
  tEnd: null,
  sort: "last",
  order: "desc",
};

export function filterToQuery(f: SessionsFilter): URLSearchParams {
  const p = new URLSearchParams();
  if (f.projects.length > 0) p.set("project", f.projects.join(","));
  if (f.tools.length > 0) p.set("tools", f.tools.join(","));
  if (f.subagents !== "any") p.set("subagents", f.subagents);
  if (f.tStart) p.set("tStart", f.tStart);
  if (f.tEnd) p.set("tEnd", f.tEnd);
  if (f.sort !== "last") p.set("sort", f.sort);
  if (f.order !== "desc") p.set("order", f.order);
  return p;
}

export function queryToFilter(p: URLSearchParams): SessionsFilter {
  const csv = (k: string) =>
    (p.get(k) ?? "")
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean);
  const subagents = p.get("subagents");
  const sort = p.get("sort");
  const order = p.get("order");
  return {
    projects: csv("project"),
    tools: csv("tools"),
    subagents:
      subagents === "yes" || subagents === "no" ? subagents : "any",
    tStart: p.get("tStart"),
    tEnd: p.get("tEnd"),
    sort:
      sort === "cost" || sort === "tokens" || sort === "started"
        ? sort
        : "last",
    order: order === "asc" ? "asc" : "desc",
  };
}

export function filterToServerQuery(f: SessionsFilter): URLSearchParams {
  const p = filterToQuery(f);
  p.delete("tStart");
  p.delete("tEnd");
  return p;
}

export async function fetchSessions(f: SessionsFilter): Promise<SessionRow[]> {
  const qs = filterToServerQuery(f).toString();
  const r = await fetch(`/api/sessions${qs ? "?" + qs : ""}`);
  if (!r.ok) throw new Error(`fetch sessions: ${r.status}`);
  return (await r.json()) as SessionRow[];
}

export interface ProjectRow {
  key: string;
  display: string;
  sessionCount: number;
}

export async function fetchProjects(): Promise<ProjectRow[]> {
  const r = await fetch("/api/projects");
  if (!r.ok) throw new Error(`fetch projects: ${r.status}`);
  return (await r.json()) as ProjectRow[];
}

export interface SessionsMeta {
  earliest: string | null;
  latest: string | null;
  tools: string[];
}

export async function fetchSessionsMeta(): Promise<SessionsMeta> {
  const r = await fetch("/api/sessions/meta");
  if (!r.ok) throw new Error(`fetch sessions meta: ${r.status}`);
  return (await r.json()) as SessionsMeta;
}

# Dashboard Design

**Date:** 2026-04-15  
**Status:** Approved

---

## Goal

Add an analytics dashboard as the primary view of the Claude Usage Visualizer, surfacing the most actionable cost-optimization and usage signals from the investigation reports (`test/usage-optimization-report.md`, `test/usage-optimization-report-v2.md`). The existing transcript viewer becomes secondary navigation.

---

## Actionable signals to surface

Ranked by $ impact from the investigation reports:

| Signal | Lever | Where shown |
|---|---|---|
| Top sessions driving tail cost (top 25 = 22% of spend) | $3,956 | Top sessions table |
| Agent calls inheriting Opus (41% have no explicit model) | $500–1,500 | Agent model panel |
| Cross-session file re-reads (top file: 256 sessions) | $800–2,000 | File hotspots table |
| Cache hit rate (cache_read / total input) | structural | Cache health panel |
| Cache thrash turns (high cc_tokens, ~0 output) | $200–500 | Cache health panel |
| High-error sessions cost 58% more $/turn | $250–400 | Error summary panel |
| Session cost distribution (tail vs bulk) | visibility | Session distribution table |

**Explicitly excluded:** 1h vs 5m cache tier split — hardcoded in the Claude binary, not user-controllable.

---

## Navigation & structure

`web/index.html` restructured as a unified SPA. Top nav:

```
[Dashboard]  [Transcripts]        [7d | 30d | 90d | All]   [Project selector ▾]
```

- **Dashboard** is the default landing view.
- **Transcripts** shows the existing project/session/timeline view, unchanged.
- **Time range** applies to all dashboard panels. Default: 30d.
- **Project selector** defaults to "All Projects". Selecting a project scopes both the dashboard and transcript views. A breadcrumb `All Projects › ~/path/to/project` appears with a back link.

State held at App level: `{ activeTab, timeRange, selectedProject }`. All panels fetch their own data when these change.

---

## Dashboard panels

### Global view (All Projects)

**1. Summary bar** — 4 stat cards:
- Total spend
- Sessions (main · subagents)
- API calls (deduped billing events)
- Avg cost/session

**2. Daily spend chart** — Recharts `BarChart`, stacked by model (Opus / Sonnet / Haiku). One bar per day over the selected time range.

**3. Model breakdown table** — columns: model, sessions, API calls, total cost, % of spend, avg $/turn.

**4. Cache health panel** — 3 metrics:
- **Cache hit rate** — `cache_read_tokens / (input + cache_read + cache_create)`. Low = context churns, caching not effective.
- **Cache create rate** — `cache_create_tokens / total_tokens`. High = lots of fresh context; signals in-session re-reads.
- **Top thrash turns** table — top 10 turns by `cache_creation_input_tokens` where `output_tokens < 200`. Shows entry ID, session, cost, cc_tokens, output_tokens. These are $4–7 turns producing near-nothing.

**5. Agent model panel** — 2 stats + table:
- Explicit model calls vs inherited (no `model` field set)
- Cost attributed to inherited calls
- Table of top inherited subagent subtypes (e.g. `Explore`, `read-only-researcher`) — these are prime candidates for Sonnet downgrade.

**6. Top sessions table** — top 15 by cost: project, started\_at, cost, turns, errors, subagent\_count. Each row links to that session in the Transcripts view.

---

### Per-project view (project selected)

**1. Summary bar** — same 4 cards, scoped to project + time range.

**2. Session cost distribution table** — buckets matching the v1 report:

| Msgs | Sessions | Total $ | Avg $ | Max $ |
|------|----------|---------|-------|-------|
| <20 | … | … | … | … |
| 20–100 | … | … | … | … |
| 100–500 | … | … | … | … |
| 500–2k | … | … | … | … |
| 2k+ | … | … | … | … |

**3. File hotspots table** — top 30 files by distinct session read count. Columns: file path, distinct sessions, total reads. These are canonical context candidates — files that should be summarized once rather than re-read each session.

**4. Error summary panel** — two parts:
- Error type breakdown: `permission_denied`, `tool_use_error`, `no_such_file`, `timeout`, `other` — count + sessions affected.
- $/turn by error bucket table (0 errors / 1–9 / 10–49 / 50+) — shows the 58% $/turn premium in high-error sessions.

---

## Charting library

**Recharts** via CDN:

```html
<script src="https://unpkg.com/recharts/umd/Recharts.js"></script>
```

Used for: `BarChart` (daily spend), `AreaChart` or `LineChart` (trends if added later). All other panels use plain HTML tables and stat cards — no charting overhead for data that reads better tabular.

---

## API endpoints

All `GET`. Time range via `?from=<ISO>&to=<ISO>`. Project-scoped endpoints also accept `?project=<key>`.

### Global

| Endpoint | Response shape |
|---|---|
| `GET /api/dashboard/summary` | `{ cost_usd, session_count, subagent_count, api_call_count, avg_cost_per_session }` |
| `GET /api/dashboard/daily` | `[{ date, cost_opus, cost_sonnet, cost_haiku }]` |
| `GET /api/dashboard/models` | `[{ model, sessions, api_calls, cost_usd, pct_spend, avg_cost_per_turn }]` |
| `GET /api/dashboard/cache` | `{ hit_rate, create_rate, cache_read_tokens, cache_create_tokens, total_tokens, thrash_turns: [{ entry_id, session_id, project, cost_usd, cc_tokens, output_tokens }] }` |
| `GET /api/dashboard/agents` | `{ explicit_calls, inherited_calls, inherited_cost_usd, subtypes: [{ subtype, count, cost_usd }] }` |
| `GET /api/dashboard/top-sessions` | `[{ session_id, project, started_at, cost_usd, turn_count, error_count, subagent_count }]` (default limit=15) |

### Per-project

| Endpoint | Response shape |
|---|---|
| `GET /api/dashboard/project-summary` | same shape as global summary |
| `GET /api/dashboard/session-distribution` | `[{ bucket, session_count, total_cost, avg_cost, max_cost }]` |
| `GET /api/dashboard/file-hotspots` | `[{ file_path, distinct_sessions, total_reads }]` (default limit=30) |
| `GET /api/dashboard/errors` | `{ types: [{ error_type, count, sessions_affected }], by_bucket: [{ bucket, sessions, avg_cost_per_turn, errors_per_turn }] }` |

---

## Component tree

```
App
├── Header (tab nav, time range picker, project selector)
├── DashboardView
│   ├── GlobalDashboard  (selectedProject === null)
│   │   ├── SummaryBar
│   │   ├── DailySpendChart          ← Recharts BarChart
│   │   ├── ModelBreakdownTable
│   │   ├── CacheHealthPanel
│   │   ├── AgentModelPanel
│   │   └── TopSessionsTable
│   └── ProjectDashboard  (selectedProject set)
│       ├── SummaryBar
│       ├── SessionDistributionTable
│       ├── FileHotspotsTable
│       └── ErrorSummaryPanel
└── TranscriptView  (existing — unchanged)
```

---

## Out of scope

- 1h vs 5m cache tier visualization (not user-controllable)
- Authentication or multi-user support
- Auto-refresh / live polling
- Export to CSV
- Per-skill dollar attribution (requires complex windowing)

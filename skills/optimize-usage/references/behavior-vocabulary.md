# Behavior vocabulary

Every recommendation must map to a verb from this list. If your proposed action does not map here, it is not yet a behavior — it's a description. Rewrite.

The point of closing the vocabulary is that all of these are things a Claude Code user can actually do with their hands in an afternoon. "Reduce cache-read tax" is not on this list because there is no hand-action for it. "Trim global CLAUDE.md" is — the user opens the file and deletes lines.

---

## Verb list

### Session-level (habit / in-the-moment)

- **End session at N turns** — set a personal habit; optional Stop-hook enforcing it.
- **`/clear` at task boundary** — new task, new context.
- **Split a session by project** — switch cwd → new session.
- **New session per subagent batch** — don't chain orchestration within one session.
- **Avoid 1M-context mode unless needed** — default to 200k; flip per-task.
- **Stop pasting X into chat** — where X is a large document or screenshot, replace with pointer.

### Configuration-level (one-time edits)

- **Trim CLAUDE.md section X to Y tokens** — concrete file + section + target size.
- **Move skill X out of always-load** — from `~/.claude/skills/` to plugin- or project-scoped.
- **Delete skill Z** — user is no longer using it.
- **Fork skill to shorter variant** — copy + shrink; keep original for cases that need it.
- **Remove MCP server Y** — delete from `.mcp.json` or `~/.claude.json`.
- **Replace chatty MCP with CLI wrapper** — when Claude's tool-wrapper returns 20k chars that a `--json | jq` slice reduces to 2k.
- **Lower extended-thinking budget** — env var adjustment.
- **Disable PostToolUse hook H** — or narrow the hook's trigger conditions.

### Delegation-level (agent spawn hygiene)

- **Add `model: sonnet` to subagent-type X** — explicit model override in the Agent call.
- **Collapse N reviewers to 1** — cut redundant spawns in a skill body.
- **Replace spawn with inline work** — when the subagent's cold-cache tax exceeds the main-chain marginal cost.
- **Cap subagent turn count** — where the skill supports it.
- **Serialize what was parallel** — for tasks where latency doesn't matter.

### Artifact-level (how big things flow)

- **Replace embed with path-pointer** — Agent prompts say "read plan at X" instead of quoting the plan.
- **Cap plan size at N tokens** — edit the plan-writing skill to refuse to produce larger.
- **Two-tier artifact (summary + detail appendix)** — subagents read only the summary by default.
- **Stop writing doc type X** — when the doc is ceremonial and nothing downstream reads it.
- **Narrower Read with offset/limit** — rather than reading whole large files.
- **Grep → Read** — locate first, then read a slice.

### Workflow-level (redesigns)

- **Batch tasks A+B into one session** — when setup cost is amortized.
- **Stop running /command in a loop** — or put it on a wider cadence.
- **Change the default skill for task type** — a different skill is cheaper for the same job.
- **Abandon the workflow** — honest option when the cost exceeds the value.

---

## Mechanism → candidate behaviors lookup

Pick 1–3 candidate behaviors per row by feasibility for this user. Every mechanism has at least one low-effort option.

| Mechanism finding | Candidate behaviors |
|---|---|
| main-chain cache-read dominant, long sessions | End sessions at <N turns; trim CLAUDE.md; move always-load skill X out; `/clear` at task boundaries |
| main-chain cache-read dominant, fat context | Avoid 1M-context mode; narrower Reads; summarize external docs |
| main-chain cc5m at gap<5min (true invalidation) | Stop mid-session CLAUDE.md edits; disable plugin-reconnect hook; identify the invalidation trigger and remove |
| sidechain cc5m dominant (cold subagents) | Add `model: sonnet` to subagent-type X; collapse reviewer fan-out; replace spawn with inline |
| Big Write outliers on plans | Cap plan size; replace in-plan quotes with path+lineno pointers; two-tier plan format |
| Huge Agent prompt lengths | Shorten prompts to pointers; single reviewer instead of multi |
| Agent spawns with inherited Opus | Add explicit `model: sonnet` on the spawn |
| Output dominant with thinking blocks large | Lower extended-thinking budget |
| Output dominant without thinking (plain generation) | Shorten workflow instructions; stop writing ceremonial docs |
| First-turn cc very high (big system prompt) | Remove unused MCP servers; move skills out of always-load; trim CLAUDE.md |
| MCP tool_result sizes huge | Replace chatty MCP with CLI wrapper; narrow MCP tool's args |
| Hook-injected content large | Disable or narrow PostToolUse hook |
| Recurring compaction in same sessions | `/clear` at task boundary instead of letting auto-compact fire |
| Regular off-hours spawns (loops) | Stop / widen cadence of the loop / cron job |
| MCP-codex or external-LLM response re-ingest | New session after external-LLM chunk to avoid forever-ingesting the reply |

---

## Framing rule

Every recommendation is written verb-first. Mechanism comes second, as the reason.

- ❌ "Cache-read tax dominates on long sessions."
- ✅ "End sessions at 150 turns. Why: long sessions re-pay cache-read on the full prefix every turn; current p90 is 340 turns, driving 58% of weekly spend."

- ❌ "MCP schema bloat inflates first-turn cache creation."
- ✅ "Remove the Grafana MCP from this project's `.mcp.json`. Why: its tool schemas add ~4k tokens to every fresh session's system prompt and you haven't invoked any Grafana tool in 30 days."

If the verb-first version is hard to write, the behavior isn't concrete enough. Narrow until it is.

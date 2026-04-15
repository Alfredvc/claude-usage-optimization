# Archetypes

These are canned hypothesis clusters that map workflow signals to candidate root causes and behaviors. Same symptom ("bill too high") has different root causes per workflow shape. A user can match zero, one, two, or all of these — never force one.

An archetype matches when the signal is **distributional**, not when a single session crosses a threshold. "12 of 50 sessions have >200 turns" is not long-session archetype unless those 12 drive a meaningful share of cost (check p50 and p90, not just any-cross).

When an archetype matches, its levers become pre-committed recommendations in Phase 5. You may drop one with evidence from Phase 4, but if you drop it, state why in "What I could not rule out".

---

## A. Plan-heavy workflow (plans + subagent execution)

**Symptom signals.**
- cache-creation dominant (from stream-separated token-type probe)
- many short subagent sessions (high sidechain session count, low sidechain per-session turn count)
- writes to `**/plans/**`, `**/docs/plans/**`, skill-defined plan dirs
- plan file sizes >5k tokens on average

**Likely roots.**
- Massive plan artifacts re-ingested into every subagent spawn
- Agent prompts that quote the plan rather than pointing at its path
- Multiple reviewer/critic agents each ingesting the same plan
- Plan Mode (`ExitPlanMode`) producing a plan that then propagates into subsequent turns

**Discriminating probes.**
- Artifact end-to-end cost on plan paths (probes.md #18)
- Subagent Agent prompt length distribution (probes.md #8)
- Count of reviewers per plan (group Agent calls by a rough task clustering)

**Levers (ordered low → high effort).**
- Low: cap plan size at N tokens in the plan-writing skill (one-line edit)
- Low: add `model: sonnet` to reviewer subagent-types where judgment isn't critical
- Medium: teach subagents to read the plan by path once, not quote it in the Agent prompt
- Medium: collapse N reviewers into a single reviewer
- High: fork the plan-writing skill to produce a two-tier output (high-level + detail appendix) so subagents can read only what they need

---

## B. Agent-team-heavy workflow (broad parallel delegation)

**Symptom signals.**
- High sidechain share of total spend (>40%)
- Many parallel Agent calls per main-chain turn
- Short turns per subagent but high spawn volume
- Subagent-inheritance probe shows many `explicit_model IS NULL` spawns

**Likely roots.**
- Subagents inheriting parent's Opus when Sonnet would suffice
- Long Agent prompts repeated across spawns (book-length briefings)
- Redundant spawn patterns (same question to N agents)
- `subagent-driven-development` or similar skills that emit 4-5 parallel calls by default

**Discriminating probes.**
- Agent spawn model reality (probes.md #11) — get both explicit and actually-executed model
- Agent prompt length histogram — if the p50 is >2k tokens, briefings are bloated
- Subagent-type frequency ranking — identify which types dominate volume
- Cost per spawn by subagent-type

**Levers.**
- Low: add `model: sonnet` to subagent-types that don't need judgment (identify by looking at what the subagent actually does — file searches, linting, test runs = Sonnet)
- Low: shorten Agent prompts with pointers ("read plan at path X, review section Y") instead of quoting
- Medium: collapse redundant spawns — if the skill always spawns 4 agents with slight variations, often 1 with a richer prompt is cheaper
- Medium: set subagent turn caps where the skill supports it
- High: fork the orchestrating skill to default to serial instead of parallel for tasks where latency doesn't matter

---

## C. Long-session / fat-context workflow

This is really two archetypes. Split them — the lever differs.

### C1. Long-session (many turns, normal context size)

**Symptom signals.**
- cache-read dominant on main-chain
- Sessions >200 turns account for a meaningful cost share (not just one outlier)
- Per-session cost growing over time even as context-size-per-turn is flat

**Likely roots.**
- Big CLAUDE.md + many always-loaded skills + MCP schemas cache-read every turn × many turns
- No `/clear` discipline at task boundaries
- Compaction cycles firing mid-session (probes.md #15) — cold-cache tax
- Session habits: one big session per day instead of task-scoped sessions

**Levers.**
- Low: trim global CLAUDE.md, especially volatile sections near the top
- Low: move rarely-used skills out of `~/.claude/skills/` (always-loaded path) to plugin-scoped or project-scoped
- Low: remove MCP servers not needed for the current project (a Grafana MCP on a pure-frontend project is pure tax)
- Medium: `/clear` after each distinct task; new session per feature
- Medium: cap session turn count with a habit or hook
- High: redesign workflow into shorter, narrower sessions with explicit context handoff

### C2. Fat-context (few turns, enormous per-turn context)

**Symptom signals.**
- Sessions crossing 200k tokens (1M-context 2× pricing tier)
- High cost-per-turn on a small number of sessions
- Large attachments, large tool_results, or aggressive context-gathering

**Likely roots.**
- Pasting large documents / PDFs / screenshots
- `Read` of very large files or `WebFetch` of huge pages
- 1M-context mode used for tasks that didn't need it

**Levers.**
- Low: avoid 1M-context mode unless genuinely needed (every turn is 2×)
- Low: use `offset` / `limit` on Read for big files; extract with Grep before Read
- Medium: summarize large external documents before introducing them
- Medium: use `Glob`/`Grep` to locate, then Read narrowly

---

## What if the user matches none of these?

Likely possibilities:
- **MCP-invisible-tax user.** Large MCP schemas + hook-injected content driving first-turn cc with short human sessions. Probe: system-prompt size (probes.md #3) and MCP tool_result sizes (probes.md #4).
- **Autonomous loop user.** Background tasks / schedules firing while the user is away. Probe: autonomous loop detection (probes.md #14).
- **Extended-thinking user.** Thinking-heavy configuration on every turn. Probe: thinking tokens (probes.md #12).
- **External-LLM feedback user.** Codex MCP or similar tools whose responses re-ingest forever. Probe: MCP tool_result sizes filtered to the relevant prefix.

If none of the archetypes match and none of these fallbacks illuminate the spend, return to Phase 1 mechanism sweep and re-rate. Almost always the pre-commit list was too narrow.

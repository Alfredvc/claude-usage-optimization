# Exemplar good output

Two artifacts: a longer chat summary (the main response the user sees in the terminal) and a full report written to a markdown file. Pattern-match both.

The user-facing unit is always **% of weekly usage**. Dollars, tokens, and hours are investigative intermediates — keep them out of user-facing output, with two exceptions: (a) the absolute size of an artifact you're asking the user to shrink (e.g. "11k-token CLAUDE.md") belongs in the mechanism explanation because it describes a thing, not an impact; (b) token counts in "Measured now" / "Target" are about configuration, not impact. Saving and root-cause impact are always %.

---

## Chat summary exemplar

```
## Usage audit — Mar 28 – Apr 11 (14 active days)

Baseline: roughly steady at the current weekly level. Top drivers of weekly usage:
bloated global CLAUDE.md re-cached every turn (~25%), long sessions with heavy
prefix (~20%), Agent spawns inheriting Opus on subagents that don't need it (~15%).
Exact traces and citations in the full report linked below.

## Prioritized recommendations

### 1. Trim global `~/.claude/CLAUDE.md` from ~11k → <3k tokens
What to do: Move the "Global Preferences > Frontend", "Scope discipline", and
"Aliases" sections into on-demand skill files that trigger by context. Keep only
sections that genuinely apply to every single turn in the global file.
Why it bites: CLAUDE.md sits in the prefix of every turn; it's re-cached on every
turn of every session. Your p90 session is 340 turns, so 11k tokens turns into
roughly 3.7 million re-read tokens per whale session. First-turn cache-creation
analysis confirms CLAUDE.md is the dominant contributor to your prefix.
Saving: ~8–12% of weekly usage (adoption-adjusted for a config edit).
Effort: Low (single file edit + create 3 skill files with the extracted content,
~15 minutes).
Risk: Capability loss — you may reference moved sections automatically today; the
replacement skills must trigger when relevant. Reversibility: high (git revert).
Verify in 7 active days: re-run the stream-separated token-type probe; main-chain
cache-read share should drop by roughly 10 percentage points.

### 2. End sessions at ~150 turns, `/clear` at task boundaries
What to do: Adopt a personal session-length limit and use `/clear` when you finish
a task rather than continuing in the same session. Optionally, add a Stop-hook
that reminds you at 150 turns.
Why it bites: Cache-read cost grows linearly with turn count on a fixed prefix.
6 of your top-10 most-expensive sessions crossed 200 turns and collectively account
for ~43% of main-chain usage. A controlled-for-turn analysis confirms it's the
turn count, not task difficulty — cost-per-turn at turn 200 is 3.8× cost-per-turn
at turn 20.
Saving: ~6–10% of weekly usage (adoption-adjusted for a habit change; adoption
rate ~50%, so the range is already discounted).
Effort: Medium (behavioral; needs ~1 week of discipline or a hook to enforce).
Risk: Habit friction — you'll lose context on long-running tasks. Mitigate by
writing a short handoff note before clearing.
Verify in 14 active days: p90 session length should drop below 200 turns.

### 3. Add `model: sonnet` to the 4 subagent-types that don't need judgment
What to do: Edit the skill definitions for your `Explore`-style, grep-style, and
test-runner subagents to explicitly specify `model: sonnet` in the spawn. These
types do file search and narrow execution — they don't need Opus-level reasoning.
Why it bites: 89% of your Agent calls have no explicit `model` field, so they
inherit whatever your `/model` is at spawn time — usually Opus. Opus sidechain
spawns carry cold-cache cost at Opus rates. The 4 subagent-types together account
for ~60% of sidechain volume and almost none of them do work Sonnet couldn't.
Saving: ~4–7% of weekly usage (adoption-adjusted for a config edit).
Effort: Low (4 YAML-frontmatter edits, ~10 minutes).
Risk: Quality loss on edge cases where Sonnet might miss something Opus would
catch; low in practice for search/exec subagents. Reversibility: high.
Verify in 7 active days: Opus share of sidechain usage should drop sharply; total
sidechain share of weekly usage should drop by roughly 5 percentage points.

**Combined if all three adopted:** ~16–25% of weekly usage (compounded, not
summed — they overlap partially on cache-read share).

## Other recommendations in the full report

Briefer so you can scan; details and traces in the report:
- Disable the lint-output PostToolUse hook on large files — ~1–2% saving, Low effort.
- Replace the Grafana MCP server in the frontend project (unused there) — ~1–2% saving, Low effort.
- Cap plan files at 3k tokens in the plan-writing skill — ~2–4% saving, Medium effort.
- Fork subagent-driven-development to default-serial instead of parallel — ~3–6% saving,
  High effort; only worth it if you can't solve sidechain share via rec #3 alone.

## What I could not rule out

Two confounders remain open. The CLAUDE.md-share-vs-session-length split is correlated
enough that I can't cleanly attribute percentages to each root cause separately; the
combined estimate accounts for this. Also, the Grafana MCP unused-server finding assumes
you haven't started using it in the last 3 days — if you have, its saving drops toward 0.

Full report: `./claude-usage-report-2026-04-15.md` — root-cause traces with quoted
citations, falsification probes, two additional effort-tiered options per cause, and
methodology notes including which pre-committed hypotheses were REFUTED.
```

### What makes the chat summary good

- **Length is earned, not padded.** Each prioritized block gives enough detail that the user can decide whether to act on it without opening the full report.
- **Unit is % of weekly usage.** No dollars, no tokens, no plan tiers, no hours-freed.
- **Ordering is by absolute dollar impact (internal), highest first.** Saving percentages happen to agree here because baseline is fixed, but the *logic* you're applying is "biggest-fish-first", not "easiest-first".
- **"Other recommendations" exist and are named.** The user knows the full report is not empty filler.
- **Combined saving uses compounding** (`1 − ∏(1 − s_i)`), not addition.
- **"What I could not rule out"** is in the chat summary, not buried in the report.

---

## Full report exemplar: one root cause with three effort-tiered options

The full report contains a block like this per root cause, plus the surrounding structure from SKILL.md's "Full report template". Savings are stated as % of weekly usage; the $ math is the agent's internal workings, not shown.

> **Root cause: long sessions re-pay cache-read on a bloated prefix every turn.**
> Active in most recent 14 days: 6 of top-10 sessions crossed 200 turns; cache-read share on main-chain is 58% of weekly usage, up from 31% four weeks ago. Propagation: all main-chain turns, not sidechains. Falsification probe: controlled-for-turn analysis shows cost-per-turn at turn 200 is 3.8× cost-per-turn at turn 20, so it's a real cache-read effect rather than selection for harder tasks.

### Option 1 — Low effort (config edit, ~15 min)

> **Action:** Trim global `~/.claude/CLAUDE.md` from 11.2k → <3k tokens by moving the "Global Preferences > Frontend", "Scope discipline", and "Aliases" sections into separate on-demand skill files.
>
> **Why it bites:** CLAUDE.md is in the prefix of every turn; at 11k tokens × p90 340-turn session × 4 sessions/week, it dominates your prefix bloat. First-turn cache-creation analysis in probes.md #3 confirms it as the largest single contributor.
>
> **Measured now:** Global CLAUDE.md = 11.2k tokens (probes.md #3). Main-chain cache-read share = 58% of weekly usage.
>
> **Target:** Global CLAUDE.md <3k tokens. Expected main-chain cache-read share drops toward ~45% of weekly usage.
>
> **Saving:** ~8–12% of weekly usage (adoption-adjusted for a config edit, ~90% rate).
>
> **Effort:** Low (single file edit + create 3 skill files with the extracted content).
>
> **Risk:** Capability loss — you may reference some moved sections automatically; the replacement skills must trigger when relevant. Reversibility: high (git revert).
>
> **Verify:** Re-run probes.md #1 (stream-separated token-type) in 7 active days. Main-chain cache-read share should drop ~10 percentage points. If unchanged, something else is driving the prefix bloat — check MCP schemas next.

### Option 2 — Medium effort (habit shift, ~1 week to form)

> **Action:** Adopt `/clear` at task boundaries and a personal limit of ~150 turns per session.
>
> **Why it bites:** Cache-read cost grows linearly with turn count on a fixed prefix — halving session length roughly halves per-session cache-read on long-session usage share.
>
> **Measured now:** Sessions >200 turns = 6 of top-10; they account for 43% of main-chain usage. p90 session length = 340 turns.
>
> **Target:** p90 session length <150 turns. Long-session usage share <20%.
>
> **Saving:** ~6–10% of weekly usage (adoption-adjusted for a habit change, ~50% rate; the raw behavioral impact is closer to 12–20% if fully adopted).
>
> **Effort:** Medium (behavioral; needs ~1 week of discipline or a hook to enforce).
>
> **Risk:** Habit friction — you'll lose context on long-running tasks. Mitigate with explicit context handoff notes before clearing.
>
> **Verify:** Re-run probes.md #7 (turn distribution) in 14 active days. p90 should drop below 200.

### Option 3 — High effort (workflow redesign, ~1 month)

> **Action:** Fork the `subagent-driven-development` skill to default to serial execution with explicit per-step checkpoints instead of parallel fan-out.
>
> **Why it bites:** Your sidechain usage share is 47%, driven by Agent spawns averaging 3.2k-token briefings × 18 spawns/active-day × cold-cache tax at Opus rates. A serial default amortizes the briefing across fewer spawns and allows Sonnet downgrades mid-sequence.
>
> **Measured now:** Sidechain share = 47% of weekly usage. Average Agent prompt = 3.2k tokens. 89% of Agent calls have no explicit `model` (inheriting parent Opus).
>
> **Target:** Sidechain share <25%. Average Agent prompt <1.5k tokens (pointer-based).
>
> **Saving:** ~5–9% of weekly usage (adoption-adjusted for a workflow redesign, ~30% rate).
>
> **Effort:** High (fork skill, rewrite instructions, adjust to slower cadence).
>
> **Risk:** Quality loss on tasks that genuinely benefit from parallel diverse perspectives. Also capability loss on tasks currently handled well by the original skill.
>
> **Verify:** Re-run probes.md #1 in 21 active days. Sidechain share should drop below 30%. If quality suffers, fork again with a narrower scope.

---

## What makes this exemplar GOOD

1. **Verb-first.** Each action starts with a concrete verb from `behavior-vocabulary.md` ("Trim", "Adopt", "Fork").
2. **Measured anchor.** Every "Target" has a "Measured now" counterpart from a named probe — no round numbers pulled from air.
3. **Bracketed savings as % of weekly usage.** Every range is discounted by adoption rate, explicitly stated. No dollars, no tokens, no hours in the impact number.
4. **Ordered by absolute $ impact (internal), presented as %.** The ordering principle is biggest-fish-first; the user-facing unit is %.
5. **Effort-tiered options.** Three levels so the user can choose. Not all-or-nothing.
6. **Falsification acknowledged.** The root-cause summary includes what Phase 4 probe ruled out.
7. **Verification named.** Each option points at the exact probe and the direction of change, with an "if unchanged" escape hatch.
8. **Risk tagged by type.** Capability loss / habit friction / quality loss — not "minimal".
9. **Mechanism second, not first.** The "Why it bites" line is one sentence of mechanism as the *reason*, not the headline.

## What a BAD version would look like (anti-pattern)

> "Cache-read is dominant at 58%. Consider shortening sessions to reduce cache-read tax. Also CLAUDE.md is large."

Problems: no verbs the user can act on, no measured numbers, no target, no saving range, no effort, no risk, no verification, mechanism presented as finding, no ordering, no pointer to full report.

---
name: optimize-usage
description: Investigate a user's Claude Code setup to find ROOT causes of spend and recommend cost optimizations. Use whenever the user asks to "reduce my Claude cost", "why is Claude so expensive", "optimize my usage", "analyze my spend", "audit my setup", "what's driving my bill", "cost insights", or any variant of diagnosing their Claude Code habits and configuration. This is a diagnostic methodology skill — it sits on top of `claude-usage-db` (which covers raw SQL) and guides the investigation itself so the agent doesn't declare victory on shallow findings. Trigger even if the user only mentions part of this (e.g. "my Opus bill is high" or "are my skills expensive?") — partial cost questions still need the full root-cause discipline.
---

# Optimizing a user's Claude Code usage cost

## What this skill is for

Investigating a user's Claude Code setup (via the transcripts DuckDB at the repo root) to uncover the **root causes** of their cost, and recommending targeted optimizations. The goal is not a cost breakdown; the goal is **to change their bill**.

This is hard. Category rollups ("cost by model", "cost by project") are trivial to produce and trivially misleading. They tell you where money went but not why. The why lives in the user's workflow — which skills they run, what artifacts those skills produce, how big those artifacts are, how many subagents those artifacts propagate to, what sits in context for what fraction of the session. That chain is what you have to reconstruct.

This skill is a methodology. It assumes you already know how to query the DB (see `claude-usage-db` in this same skills directory). If you have not read that skill, **read it first** — especially the billing-safety section. Every cost aggregation must use `assistant_entries_deduped` with `message_id IS NOT NULL`, or your numbers are ~2× too high and your recommendations will be proportionally wrong.

---

## Why this skill exists (failure modes to avoid)

Without discipline, agents reliably produce bad cost investigations. The failures are predictable:

**1. Category-sum trap.** Running `SUM(cost) GROUP BY model`, then `GROUP BY cwd`, then `GROUP BY tool_name` and stopping when one bucket looks big. This finds which _bucket_ is expensive; it does not find _why_ that bucket is expensive, which is what the user can act on. "Opus is 94% of spend" is true and useless on its own — the actionable question is _what is Opus being used for that could be downgraded_.

**2. Artifact blindness.** Treating tool calls, skills, and subagents as opaque counts. "Skill X was invoked 91 times" is data. "Skill X produces a 30,000-token artifact that gets re-ingested by every downstream subagent" is a finding. The skill is the name; the artifact is the cost driver. Always follow the artifact.

**3. Victory declaration.** Finding the first large lever and stopping. Real setups have stacked problems — model choice, artifact size, context bloat, workflow cycles — and fixing only the biggest often leaves the #2 and #3 worth more combined. Plan to find at least three levers before you stop.

**4. Selection-bias sloppiness.** "Sessions that used plans cost 4.5× more" sounds damning. But plans are used for harder tasks, so harder work also gets measured in there. Before blaming plans, you have to bound how much is the plan overhead vs. the task difficulty. Correlation is not a root cause.

**5. Skipping the user.** The DB shows what happened; it does not show the user's intent, their CLAUDE.md, their skills, or their workflow. Before the deep dive, ask. A 5-minute interview saves an hour of wrong hunches.

**6. Billing footgun.** Using raw `assistant_entries` doubles your numbers. Absolute $ will be wrong; ratios _mostly_ survive but not always (multi-block responses are not uniformly distributed across models). Always use the deduped view.

---

## Methodology: five phases, in order

Each phase has a gate. Do not advance until the gate passes.

### Phase 1 — Establish the canvas

Before opening the DB, get oriented:

- Confirm the DB location and that it's current (`MAX(last_timestamp)` from `transcripts` — is it recent?).
- Read `~/.claude/CLAUDE.md` (global) and any `CLAUDE.md` in the user's active projects. These encode workflow overrides that change the meaning of downstream findings. (Example: a CLAUDE.md that mandates "dual reviewers for every plan" means plan cost is structurally amplified 2×, independent of the plan skill itself.)
- List installed skills (`~/.claude/skills/`, plugin skill dirs) and any project-level skills. Note which ones are workflow-shaping (brainstorming, plan-writing, TDD, subagent-driven-development) vs. content-shaping (frontend-design, caveman).
- Ask the user 3–5 short questions. Suggested:
  - "What's your typical workflow — do you write plans before coding?"
  - "Which projects are you most active in right now?"
  - "Any skills or subagents you know you rely on heavily?"
  - "Have you already tried anything to lower cost? What didn't work?"
  - "Any constraints — e.g. can't downgrade from Opus on task X?"

**Gate:** you can describe the user's workflow in 2–3 sentences without looking at the DB. If you can't, ask more.

### Phase 2 — Scan for anomalies, not categories

The first DB pass is **outlier-first, not aggregate-first**. You are trying to find what is _unusually large_, not what the total is. Category rollups come later.

Required queries (billing-safe — deduped view, `message_id IS NOT NULL`):

- **Biggest individual tool artifacts.** Top 30 `Write` calls by content length; top 30 `Read` calls by file size (read count × approximate file size); longest `Agent` prompts; longest bash commands. This is where the "it writes a book" patterns surface.
- **Most-repeated reads of the same file within the same session.** Wasted re-ingestion is a pure-waste indicator.
- **Most-written-to paths.** Includes plans, drafts, scratch files. Anything rewritten 3+ times is worth explaining.
- **Cost-per-message outliers.** Top 1% most expensive individual assistant turns and what they contain (tool results? thinking? long outputs?).
- **Session-length distribution.** Long sessions (>200 turns) are linear-cost but their context grows super-linearly in cache churn. Get the turn histogram.
- **Subagent-type frequency and prompt size.** Agent calls with no `subagent_type` inherit the parent's (usually Opus) model — hunt those specifically.

Then, and only then, do the aggregate rollups (model, project, tool, sidechain vs. main, day). Use these to _rank the anomalies_ you already found, not to find new ones.

**Gate:** you have written down a list of ≥5 specific anomalies (file paths, tool patterns, session IDs, subagent types). If your list is model/project/tool totals only, you have not done this phase.

### Phase 3 — Follow each anomaly to its root

For every anomaly from phase 2, trace it end-to-end:

- **Where does it originate?** Which skill, slash command, subagent, or workflow produces it? Check `attachment_invoked_skills`, `tool_uses` with `name='Skill'`, CLAUDE.md rules, plan file locations (`**/plans/**`, `docs/plans/**`, etc.).
- **What does it cost per occurrence?** Not just total — the marginal cost of one instance. A 30k-token artifact ingested fresh into an Opus subagent costs ~$0.56 in cache creation per spawn. Total cost = marginal × frequency; know both.
- **Where does it get re-ingested?** Does it land in subagent contexts? In reviewer contexts? Does it get Read multiple times in the same session (each Read creates a new attachment entry and a fresh cache-create charge)?
- **What does it substitute for?** Would a pointer (file path + line numbers) do the same job at 1/10th the size? Would a smaller model handle it? Could the work be batched?

Write a one-paragraph "trace" per anomaly. The trace names the origin, the per-occurrence cost, the propagation, and the cheapest plausible alternative.

**Gate:** you can point to a specific skill, rule, or workflow pattern as the origin of each anomaly, and give a rough marginal-cost number. "I don't know what's creating this" means you haven't finished — keep digging (Read the skill body; Read the CLAUDE.md section; inspect the transcript line with `claude-usage-db`'s line-extraction recipe).

### Phase 4 — Challenge your own conclusions

Before presenting, force these checks:

- **Selection bias.** For every "sessions with X cost more than sessions without X," ask whether X is correlated with task difficulty. Test this by controlling for session length or message count. If the delta survives the control, it's real overhead. If it shrinks to near zero, X is a proxy for "hard task", not a cost driver.
- **Alternative explanations.** For each root cause you've named, write the strongest counter-argument. "Plans cost X" — could it be that _people who write plans also let sessions run longer_, and it's the session length that matters? If you can't rule it out, say so in the recommendation.
- **Compounding vs. additive.** Two changes that each save 30% do not save 60%. If a user downgrades to Sonnet AND shrinks plans, the plan savings apply to the Sonnet cost, not the Opus cost. Order and multiply; don't add.
- **Billing recheck.** One more time: are the numbers from `assistant_entries_deduped` with `message_id IS NOT NULL`? Did any query you quoted use the raw table? Redo any that did.
- **What surprised you?** If nothing in the investigation surprised you, you probably confirmed priors instead of investigating. Go back to phase 2 and hunt harder.

**Gate:** you have at least one place where you say "I can't rule out [alternative]" and at least one number you corrected from an earlier draft. If the investigation was frictionless, it was shallow.

### Phase 5 — Present with calibration

The output is not a report; the output is a decision aid.

Structure:

```
## Current spend (period, total, big buckets)
Short. Numbers. Billing-safe.

## Root causes, ranked by $ impact
For each: what it is, where it originates, rough $/period, the alternative, estimated savings with explicit uncertainty. No single-point savings numbers — always a range.

## What I could not rule out
The confounders and alternative explanations that remain open.

## Recommendations, ranked
Ordered by $ impact × effort. Each recommendation: concrete action (skill to fork, rule to add, model to downgrade, session habit to change), expected savings range, risk (what could go wrong), and what the user should verify themselves.

## What I did not investigate
Explicit. Deep data you didn't look at, projects you skipped, periods you ignored.
```

Never present a recommendation as a certainty. Every estimate is a range. Every cause is "likely" unless the counter-argument has been ruled out by data. The user is smarter than you are about their own workflow — leave room for them to reject.

---

## Stop-signals in your own reasoning

If you catch yourself thinking any of these, stop and recheck. They are the thoughts that produce shallow investigations.

- "The Opus share is huge, that's obviously the issue." (It's the bucket. What's _in_ the bucket?)
- "Found the cause, let me write the recommendation." (You found _a_ cause. Phase 2 said find five anomalies.)
- "I'll just run the standard cost-by-model query first." (Aggregates first = category trap. Outliers first.)
- "It's probably just long sessions." (Probably = alternative explanations unchecked.)
- "The user already thinks X, my data agrees, we're done." (Confirmation, not investigation. What does _not_ agree with X?)
- "Close enough on the numbers." (Are you using the deduped view? Did you filter synthetic rows?)
- "The user didn't ask about Y, skip it." (They asked you to find root causes. You choose the surface.)

These thoughts are the signal to go deeper, not the signal to wrap up.

---

## Query patterns organized by investigative intent

The `claude-usage-db` skill has the raw schema and billing-safe totals. Below are patterns organized by what you are _trying to learn_, which is the orientation this skill needs.

### "What artifacts is this user producing, and how big are they?"

```sql
-- Writes by file path, with size distribution
SELECT tu.input_file_path AS path,
       COUNT(*)                                                     AS writes,
       ROUND(AVG(LENGTH(json_extract_string(tu.input,'content')))/1000, 1) AS avg_k_chars,
       ROUND(MAX(LENGTH(json_extract_string(tu.input,'content')))/1000, 1) AS max_k_chars
FROM tool_uses tu
WHERE tu.name = 'Write' AND tu.input_file_path IS NOT NULL
GROUP BY 1
HAVING MAX(LENGTH(json_extract_string(tu.input,'content'))) > 20000
ORDER BY max_k_chars DESC LIMIT 30;
```

### "What is being read repeatedly within the same session (wasted re-ingest)?"

```sql
WITH r AS (
  SELECT e.session_id, tu.input_file_path, COUNT(*) AS reads
  FROM tool_uses tu JOIN entries e USING(entry_id)
  WHERE tu.name = 'Read' AND tu.input_file_path IS NOT NULL
  GROUP BY 1,2
  HAVING COUNT(*) > 1
)
SELECT input_file_path, SUM(reads - 1) AS wasted_reads, MAX(reads) AS peak
FROM r GROUP BY 1 ORDER BY wasted_reads DESC LIMIT 30;
```

### "Which Agent spawns inherit Opus by omission?"

```sql
SELECT json_extract_string(tu.input,'subagent_type') AS subtype,
       json_extract_string(tu.input,'model')         AS explicit_model,
       COUNT(*) AS calls
FROM tool_uses tu
WHERE tu.name = 'Agent'
GROUP BY 1, 2
ORDER BY calls DESC;
```
Any row with `explicit_model IS NULL` inherits the parent's model. If the parent is Opus, every one of those calls is Opus.

### "What do the most expensive individual turns contain?"

```sql
SELECT e.session_id,
       e.entry_id,
       d.model,
       d.cost_usd,
       d.input_tokens + d.cache_creation_input_tokens + d.cache_read_input_tokens AS ctx_tok,
       d.output_tokens
FROM assistant_entries_deduped d
JOIN entries e USING(entry_id)
WHERE d.message_id IS NOT NULL
ORDER BY d.cost_usd DESC NULLS LAST
LIMIT 20;
```
Then pull the raw line for the top few (see `claude-usage-db` for `awk`/`jq` recipe) to see what drove them.

### "Does skill X actually cost more per message, controlled for length?"

```sql
WITH sess_with_skill AS (
  SELECT DISTINCT e.session_id
  FROM tool_uses tu JOIN entries e USING(entry_id)
  WHERE tu.name = 'Skill' AND json_extract_string(tu.input,'skill') = '<name>'
),
turns AS (
  SELECT e.session_id,
         ROW_NUMBER() OVER (PARTITION BY e.session_id ORDER BY e.timestamp) AS turn,
         d.cost_usd, d.model
  FROM assistant_entries_deduped d JOIN entries e USING(entry_id)
  WHERE d.message_id IS NOT NULL
)
SELECT CASE WHEN s.session_id IS NOT NULL THEN 'with' ELSE 'without' END AS grp,
       CASE WHEN turn <= 50 THEN '1-50'
            WHEN turn <= 200 THEN '51-200'
            ELSE '201+' END AS bucket,
       ROUND(AVG(t.cost_usd)*100, 2) AS avg_cents
FROM turns t LEFT JOIN sess_with_skill s USING(session_id)
WHERE t.model LIKE 'claude-opus%'  -- adjust to your dominant model
GROUP BY 1, 2
ORDER BY 2, 1;
```
This controls for turn position so you're not conflating skill-use with long-session effects.

### "What do plan files cost, end-to-end?"

Trace the artifact through the pipeline:
1. Write cost: output tokens of the Write calls on plan paths.
2. Ingest cost: distinct subagent sessions that Read a plan × plan size × cache-create rate for the model.
3. Recitation cost: Agent prompts that embed plan excerpts (check `LENGTH(prompt)` on Agent calls in plan-writing sessions).
4. Context-tax cost: once a plan is in a subagent context, every assistant turn cache-reads it (≈ plan_size × cache_read rate × turn_count).

Same template applies to any artifact (design docs, research outputs, long CLAUDE.md rules, etc.).

---

## A worked failure, for calibration

In one real investigation, an agent did cost-by-model, cost-by-project, cost-by-tool, and declared the root cause was "Opus on subagents, 49% of spend." That was a real finding — but it was not the biggest one. The user then asked about plan files; a 10-minute hunt surfaced 175 plan files averaging 7k tokens, some reaching 30k tokens, re-ingested into 412 subagent sessions covering 61% of spend, with 2,927 wasted in-session re-reads. That was the bigger lever. The agent had not looked, because the agent rolled up by category instead of hunting outliers in artifact size.

The pattern generalizes. The biggest cost drivers in a Claude Code setup are not necessarily obvious. Dig deeper, find the root cause. Involve the user if necessary.

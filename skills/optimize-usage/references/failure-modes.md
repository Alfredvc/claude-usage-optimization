# Failure modes (extended)

The top 5 live in SKILL.md. The rest are here. Read this before Phase 4 adversarial rebuttal — several of these are common traps the rebuttal should check for.

---

**6. Calendar anchoring.** Calendar weeks lie. Users take vacations, have focus blocks, switch projects. Anchor on **active days** (days with real usage) and on the user's current workflow state — not on `last 7 days`. Also: autonomous loops / crons will make a "dead" day still look active because the bot ran — cross-reference with autonomous-loop detection before trusting active-days framing.

**7. Stale findings.** A skill the user uninstalled two weeks ago is not a cause. When a candidate cause is found in older data, verify it's still active in the most recent active window before recommending anything. Key the currency check on the **artifact pattern** (path glob, output shape), not the named producing skill — a user may still produce the same artifact via a different producer.

**8. Billing footgun.** Using raw `assistant_entries` doubles numbers. Ratios mostly survive but not always (multi-block responses aren't uniform across models). Always use the deduped view with `message_id IS NOT NULL`.

**9. Selection-bias sloppiness.** "Sessions that used plans cost 4.5× more" sounds damning. But plans are used for harder tasks — so harder work also shows up. Before blaming plans, control for session length or turn count. Correlation is not a root cause. Phase 2b's "skill use controlled for session length" probe exists for this reason.

**10. Archetype lock-in from outlier sessions.** The long-session archetype signals >200 turns. A user with 48 sessions <50 turns and 2 sessions >400 turns will cross the threshold on the outliers. Check the distribution (p50 and p90), not "any crosses". An archetype should explain a meaningful cost share, not 3%.

**11. Gate-gaming via vague citations.** Phase 3 requires every anomaly have a "named origin". An agent can write "originates from CLAUDE.md global rules" without actually checking. Require an *exact* citation: line number in the file, or skill path, or entry_id in the JSONL. If it's not quotable, the trace isn't complete.

**12. Fake corrections in Phase 4.** The "at least one number changed" gate invites the agent to change a rounded number by $5 and call it done. The correction must come from a named cause (deduped-view recheck, synthetic-row filter, turn-count control, TTL-aware cache-reset bucket) — state which one.

**13. Presenting plan-tier or cap numbers to the user.** Anthropic's subscription caps are not public figures you can cite — any $-thresholds you use to decide between subscription and API framing are internal heuristics, not facts. Telling the user "you're at 47% of your Max 20× cap" invents a number. Present $ per week and, for subscription users, hours-of-Claude-usage freed per week (computed from the user's own burn rate). Check for `ANTHROPIC_API_KEY` env hints in `settings.json` and shell config to decide whether the user is on API; default to subscription framing otherwise.

**14. Bursty / project-based users.** A user who onboarded a new codebase in week 3 (huge cost spike), then settled back, will have "recent active window" cost that dramatically over- or under-represents steady state. Phase 5 output should show both recent-window and trailing-12-week distributions; flag divergence >2×.

**15. "Per-occurrence cost" without propagation accounting.** A 30k-token plan "costs $0.56 to create" but propagates into 20 subagent ingests and gets cache-read on every turn of the parent session. Per-occurrence ≠ marginal. Use the artifact end-to-end template (probes.md #18) for anything that propagates.

**16. Ignoring the user's own historical priors.** If the user had this same investigation done before, skim prior findings (investigation logs, prior PRs). Not to confirm — to see which mechanisms were already ruled out or already fixed. "This is new" vs "this came back" is informative.

**17. Treating subscription caps as if they were weekly.** Current cap structures mix weekly and 5-hour rolling windows. A user hitting the 5h cap pays nothing extra but loses productivity. If you see highly concentrated hour-of-day burn, bin into 5h rolling windows and report cap pressure there, not just weekly.

**18. Trusting the skill-invocation count instead of skill effects.** `Skill` tool invocation count tells you how often a skill ran. It doesn't tell you what *content* the skill injected. A skill may appear 3× in a session but inject a persistent 5k-token system reminder that lives forever. Check `attachment_invoked_skills` and the skill body for what it actually adds to context.

**19. Confirming that the data shape matches the user's hint.** User hints are extremely useful — but they're symptoms, not diagnoses. "I think plans are the problem" might be correct, wrong, or right for the wrong reason. Use the hint to seed one hypothesis in the pre-commit list; add six more that *don't* use the hint.

**20. Fascination tell.** If you catch yourself thinking "this is a fascinating pattern" or "interesting that the cc1h peaks correlate with X", you are investigating the mechanism for its own sake. The user doesn't want fascination. They want to change their bill. Re-anchor on which behavior this finding implies.

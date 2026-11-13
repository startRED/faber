# Harness study: what makes an agent harness good (2026-11)

Status: AI research, dated 2026-11-13. Not reviewed by the owner. Figures are directional, not decisions.

Written from 12 research agents and about 170 searches on 2026-11-13. Nothing in Faber's code changed for this study.
The rival harnesses and the proposed build order are in [harness-rivals-and-build-order-2026-11.md](harness-rivals-and-build-order-2026-11.md).

## How to read this page

- Most numbers come from search snippets and from 2026 preprints that nobody has checked end to end. The research agents
  mostly read snippets, not full pages. Treat every figure as a direction.
- Before any figure becomes a decision, Faber must measure it in its own bench, per task type.
- Priorities, in order: quality first, cost second, time third.
- Tags: [evidence] marks a measured result (a preprint, a benchmark, a study). [opinion] marks a statement by a vendor or a
  practitioner, or a claim the source did not measure. Vendor-published figures are named as such.
- Sources are listed at the end of the page, by topic. An arXiv identifier is `https://arxiv.org/abs/<identifier>`.

## What matters most

1. **Cost follows turn count.** [evidence] TurnCost (arXiv 2610.15093) reports that turn count explains about 84% of the
   cost variation of an agent, and that about 98.6% of input tokens come from the cache, because every turn resends the
   whole prefix. A second preprint (arXiv 2607.12161) reports that about 87% of cost is cache traffic, so compressing tool
   output saves little. Implication: each wake-up of the main agent is expensive, and batching reports attacks the largest
   cost source.

2. **The harness matters as much as the model.** [evidence, with a vendor claim] Several 2026 studies on Terminal-Bench
   report that the same model scores up to about 44 points apart, and costs up to about 40 times more, across harnesses.
   Factory reports 65.2% for its Droid harness against 57.6% for Claude Code with the same Opus 5.5 model. That is Factory's
   own figure, not an independent one. Implication: Faber must measure per task type, not trust a general ranking.

3. **Agents flatter their own work; cross-vendor review finds more.** [evidence] A judge is more lenient with code it wrote
   itself, and models let through 64.5% of the errors they generated (arXiv 2604.06996, 2507.02778). In a planted-error
   test (arXiv 2610.01471), a pair of reviewers from different model families found 56.7% of the errors; two reviewers from
   the same model found 42.7%. In arXiv 2607.21656 (116 tasks), Claude reviewing Codex drafts raised accuracy from 71.6% to
   89.7%, while Codex reviewing Claude drafts fell from 91.4% to 82.8%. The direction of the review matters.

4. **Externally written tests beat self-written tests.** [evidence] With human-written tests an agent solves 94.3% of
   tasks (TDFlow); when it generates its own tests, 68.0%. Telling the agent which tests a change touches cut regressions
   from 6.08% to 1.82% (TDAD). A generic instruction to "write the test first" raised regressions to 9.94% in the same study.

5. **Teams help with reading and review, and hurt parallel writing.** [opinion] Anthropic says coding is a poor fit for
   teams. Cognition reports that parallel writers make conflicting decisions. [evidence] Cursor reports that 20 locked agents
   delivered the throughput of 2 or 3. In a Cohere coding test a solo agent scored 52.1%; a team of frontier models where the
   lead delegates every stage scored 63.2% at US$510; a frontier lead with 16 cheap planning workers scored 61.8% at
   US$155, about 30% of the cost. So a team can beat a solo agent, and the cheap-planner team kept almost all of the gain.

6. **Clarifying questions help only when calibrated.** [evidence] In arXiv 2603.26233, an agent that asks only on hard tasks
   solved 69.4%; one that never asks, 61.2%; one given the full specification, 70.8%. After people answer clarifying
   questions, they review less and accept even wrong answers (180 participants, arXiv 2611.01087).

## Common complaints from Claude Code and Codex users

Ranked by frequency times severity. The "Faber today" column shows only what was
confirmed in Faber's docs or code; "verify" means not yet checked. The complaints come from user threads and issue
trackers, so they are anecdotes; their links are in the sources below.

| # | Complaint | What fixes it | Faber today |
| :- | :--- | :--- | :--- |
| 1 | Ignores CLAUDE.md / AGENTS.md; rules lost after compaction | Hard rules as engine blocks, not text; re-inject rules and the request after compaction; keep instruction files short | verify |
| 2 | Deletes files or runs `git reset --hard` without asking | Block destructive commands even in automatic mode; separate copy per agent | separate copies exist; blocking: verify |
| 3 | Says "done" without doing the work | "Done" only with captured test output or a checked artifact | a verifier exists; gate: verify |
| 4 | Quota runs out fast | Fewer turns, small context, low effort on routine work | quota pacing and Auto exist |
| 5 | Slow (5 to 10 minutes for a simple task) | Fewer MCP servers and hooks at startup; less context | verify |
| 6 | Quality drops after an update | Pin effort and reasoning explicitly | verify |
| 7 | Permission prompt fatigue | Narrow allowlists; automatic mode with destructive commands blocked | partly |
| 8 | Windows: Git Bash failures, `/c/` paths | Normalize paths; warn when Git Bash fails | Faber does not run child processes through a shell |
| 9 | Billed through the API instead of the plan (key in environment) | Show which credential is being billed; warn before use | verify |
| 10 | Sycophancy and over-engineering | Checkable limits: no new dependency, no abstraction used once | verify |
| 11 | Invents APIs | Read the exports of the installed package before calling it | verify |
| 12 | Parallel agents collide | Files reserved per task; about 4 agents at most | file reservation exists (R155) |

## Open contradictions to measure

1. **Cheap or strong planner?** The Cohere test found a cheap planner almost as good and about 70% cheaper. Gemini CLI does
   the opposite: plans with the strong model and executes with the cheap one. Measure both shapes.
2. **Cross-review direction.** One study says direction matters (Claude reviewing Codex helps; the reverse hurts). Another
   says a fresh session of the same model ties with cross-vendor review. Mixed pairs won in both.
3. **Teams in code.** Anthropic says teams do not pay off for code; the Cohere test reports a gain (63.2% against 52.1%
   for a solo agent). The difference probably lies in task type and in who may write.
4. **TDD.** Reasoning with tests helps; a generic TDD instruction hurts (see item 4 above).
5. **Best-of-N.** Almost all the gain disappears when the budget is equal. It only pays with an executable verifier.

## Sources

Read from search snippets unless the line says otherwise; the preprints are unreviewed.

- Cost: arXiv 2610.15093 (TurnCost), 2607.12161 (token reduction is not cost reduction), 2609.22114 (tool-schema filtering).
  Prompt caching economics: Anthropic pricing page, not yet opened; the cache figures seen came from secondary pages.
- Harness effect: arXiv 2610.00917, 2610.32131, 2607.22585; Factory, vendor-reported:
  https://factory.com/articles/terminal-bench-agent-comparison
- Review and self-preference: arXiv 2604.06996, 2507.02778, 2610.01471, 2607.21656.
- Tests: arXiv 2510.23761 (TDFlow), 2603.17973 (TDAD, the 6.08% to 1.82% result).
- Teams: https://www.anthropic.com/research/multi-agent-research-system (2025-06-13);
  https://cognition.ai/blog/dont-build-multi-agents (2025-06-12);
  https://cohere.com/blog/lessons-from-scaling-teams-of-frontier-coding-agents (2026-10-14);
  https://cursor.com/blog/self-driving-codebases (2026-02-05);
  https://simonwillison.net/2025/Oct/5/parallel-coding-agents (review is the limit).
- Questions and intent: arXiv 2603.26233, 2611.01087, 2502.13069.
- Instruction files: arXiv 2610.30571 (stale instruction followed 90.1% of the time; a sentence giving code precedence raised
  code-following from 4.2% to 91.4%), known only through an X thread summary
  (https://x.com/LimestoneHQ/status/2119808241563803665), paper not opened.
- Stuck recovery: arXiv 2610.11542 (a context purge plus a fixed recovery message escaped a deadlock 5 of 5 times,
  against 0 of 5 for advice alone; a known-solution test bed, very small).
- Complaints (anecdotes from issue trackers): https://github.com/anthropics/claude-code/issues/24572 (rules dropped),
  /43887 (18 files deleted), /33850 (`git reset --hard` lost two days of work), /42796 and /106878 (quality regression),
  /43332 (Windows Git Bash), /53993 (invented methods); https://github.com/openai/codex/issues/13386 (instruction file cut at
  32 KB); https://anthropic.com/engineering/claude-code-auto-mode (users accept 93% of manual prompts);
  https://dev.to/ji_ai/claude-code-in-4-git-worktrees-23-tasks-9-collisions-15x-speed-155d (9 collisions in 23 tasks).

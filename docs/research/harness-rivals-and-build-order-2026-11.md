# Rival harnesses and a proposed build order for Faber

Status: AI research, 2026-11-13. Not reviewed by the owner. Figures are directional: mostly vendor-reported or taken
from search snippets, not from pages read in full. Harness results vary by task and benchmark version, and vendor
numbers are unreplicated.

This page is a proposal. Nothing here is a decision; each build item needs a measurement on Faber's own bench before it
becomes one. The evidence behind the build order, with sources, is in [harness-study-2026-11.md](harness-study-2026-11.md).

## Rival harnesses

| Harness | How it organizes work | Its trick | Measured result | Known weakness |
| :--- | :--- | :--- | :--- | :--- |
| [Amp](https://ampcode.com/docs/models-and-subagents) | Main agent, Task subagents, a read-only "oracle", `/handoff` to a new thread | The oracle is a second opinion called on demand; threads are portable | none verified | subagents cannot be steered mid-run, the main thread blocks, threads do not nest |
| [Factory Droid](https://factory.com/news/frontier-harness) | Specialized droids on a model-agnostic harness with a router | Append-only prompt keeps the cache warm; the model writes a script that runs many tool calls, so raw files stay out of context | vendor-reported, Terminal-Bench 4: Opus 5.5 65.2% (US$4.73/task) vs Claude Code 57.6%; GPT-6.1 Sol 57.6% (US$0.79) vs Codex 54.0% | numbering every line read raised cost on some models (+8% GLM, +32% Kimi), by Factory's own account |
| [Devin](https://docs.devin.ai/get-started/first-run) | Read-only Ask mode that writes a scoped prompt, then Agent mode; Knowledge items; managed child Devins | [Memory](https://docs.devin.ai/product-guides/memory.md) is a git repo of notes consolidated every night | none verified | opaque ACU pricing; reviews cite overreaching edits |
| [OpenHands](https://openhands.dev/blog/openhands-context-condensensation-for-more-efficient-ai-agents) | Stateless agent over an append-only event log | Condenser compresses history only past a threshold, to keep cache hits | vendor: up to 2x lower per-turn cost, 54% vs 53% solved on a SWE-bench Verified subset; independent ([arXiv 2508.21433](https://arxiv.org/abs/2508.21433)): masking old tool output halves cost at equal solve rate | the default condenser calls an extra model, which adds cost |
| [opencode](https://opencode.ai/docs/agents) | Build and Plan agents plus subagents | A per-agent `steps` cap: the agent must summarize and stop after N actions | none found | Plan's read-only default is overridden by a global `"edit": "allow"` |
| [Gemini CLI](https://github.com/google-gemini/gemini-cli/blob/main/docs/cli/plan-mode.md) | Main agent delegating to subagents with their own context | Strong model plans, a cheaper one implements after approval | none verified | many anecdotes of loops, token burn and quality drops after updates |
| [Aider](https://aider.chat/docs/leaderboards/) | Architect model plans, editor model applies edits | The split cut cost: [R1 architect plus Sonnet editor 64.0% at US$13.29 vs o1 alone 61.7% at US$186.5](https://aider.chat/2025/01/24/r1-sonnet.html) (2025) | the leaderboard top (gpt-5 high, 88.0%) uses no architect | the gain shrinks as single models improve |
| [Cursor](https://cursor.com/docs/bugbot) | Agent, Ask, Plan and Debug modes; Bugbot reviews | Review rules per folder: Bugbot reads `.cursor/BUGBOT.md` walking up from the changed files | a one-run forum experiment found a plan file did not stop over-building | 20 locked agents delivered the throughput of 2 or 3 |
| [Cline](https://docs.cline.bot/core-workflows/plan-and-act) | Read-only Plan mode, then Act mode with its context; a model per mode | Checkpoints; Memory Bank of six files | none for quality | [compaction scales with the window, so with 1M it never fires](https://github.com/cline/cline/issues/14329) (12% cache hit, a US$49.63 session); Memory Bank is read on every task |
| [Roo Code](https://docs.roocode.com/features/boomerang-tasks) | Orchestrator that cannot read or write files and only delegates | Each subtask has its own history; only its summary returns | none | [no parallel subagents; handoff context can be incomplete](https://github.com/RooCodeInc/Roo-Code/issues/12330) |
| [Kilo Code](https://kilocode.ai/docs/features/custom-modes) | Agents as markdown files with `mode`, `permission`, `steps` | The orchestrator routes by each agent's `description` | none | none found |
| [Windsurf](https://docs.windsurf.com/windsurf/cascade/memories.md) | Planning agent with a todo list | Rules with activation modes (always, by file, by model, manual); global rules capped at 6,000 characters | none | none found |
| [Kiro](https://github.com/kirodotdev/Kiro/issues/11786) | Requirements, design and tasks before any code | The spec is the unit of work, with a review gate | planning first helped in practitioner reports | [a spec subagent ran 592 tool calls over about 7 hours and reported success falsely](https://github.com/kirodotdev/Kiro/issues/11786); review load is a cost |
| [Claude Code](https://docs.anthropic.com/en/docs/claude-code/goal) | Subagents in their own context; hooks; `/goal` | `/goal`: a small separate model reads only the transcript and decides if the goal is met | agent teams use about 7x the tokens of one session in plan mode ([costs](https://code.claude.com/docs/en/costs)) | rules in CLAUDE.md are dropped mid-session ([issue 24572](https://github.com/anthropics/claude-code/issues/24572)) |
| Codex | `AGENTS.md` read from the root down to the folder; `/review` with rules from a "Code Review Rules" section | The review skill reports only what the change introduced, what can be shown from code and what the author would fix | [rule-guided reviews found 98% of required findings vs 58.3% without rules](https://cubic.dev/blog/codex-code-review) (secondary source) | [`AGENTS.md` is cut at 32 KB](https://github.com/openai/codex/issues/13386); it follows a stale instruction file (arXiv 2610.30571) |

"None verified" and "none found" mean the research found no figure or weakness, not that none exists.

## Build order for Faber

The items are ordered by quality first, then cost, then time. The figures come from search snippets and have not been
checked against the papers.

1. Done gate with captured proof and a fresh reviewer. Nothing closes without captured test output. When Codex writes,
   a new Claude session reviews with a skeptical prompt. When Claude writes, a new Claude session and a Codex session
   review together, because the mixed pair found more errors. Evidence: models let through 64.5% of the errors they
   made themselves (arXiv 2604.06996, 2507.02778). Two reviewers from different families found 56.7% of planted errors;
   two from the same model found 42.7% (arXiv 2610.01471). Claude reviewing a Codex draft raised accuracy from 71.6% to
   89.7%, and Codex reviewing Claude dropped it from 91.4% to 82.8% (arXiv 2607.21656, 116 tasks). The matching
   complaint is "says done without having done". Direction of review remains open (see below).

2. Acceptance tests locked before implementation. Tests are written or fixed before the work starts. The worker cannot
   edit them, and the brief names the tests the change touches. Evidence: an agent passes 94.3% with human-written tests
   and 68.0% with tests it generates itself (arXiv 2510.23761, TDFlow). Naming the touched
   tests cut regression from 6.08% to 1.82%; the result is from arXiv 2603.17973 (TDAD). A generic
   "write tests first" instruction raised regression to 9.94% (TDAD). A generic TDD order is not advised.

3. Batched notifications, and silence detection in the engine. Several child reports become one message and one turn.
   Only the engine notices silence for 30 minutes, at no token cost. Evidence: turn count explains about 84% of the
   variation in agent cost, and about 98.6% of input comes from cache (TurnCost, arXiv 2610.15093). Compressing tool
   output saves little, because about 87% of cost is cache traffic (arXiv 2607.12161). Every wake-up of the main agent
   is expensive.

4. Hard rules as engine blocks, not text. Block destructive commands even in automatic mode. Re-inject the rules and the
   original request after compaction. Keep instruction files short. A proposed addition is the line "the
   current code is worth more than an outdated instruction". One preprint (arXiv 2610.30571) reports code followed 4.2% of the
   time without that line and 91.4% with it; it was read only through a summary. Matching complaints: instruction files
   ignored or lost after compaction, and destructive commands run without asking.

5. Step cap and an honest stop. Each worker has a step limit and a cost limit. When it hits one, it reports what it did
   and what remains, never "done". When it stalls, Faber restarts cleanly from the saved state instead of only warning.
   Evidence: recovery worked 5 of 5 times, against 0 of 5 with advice alone. That sample (arXiv 2610.11542) is very small, a known-solution test bed.

6. Lean context for child sessions. Each child gets only the tools it will use, because each declared tool costs about
   1,000 to 1,400 tokens per turn. Old tool output is masked before any AI summary, and the prompt prefix only grows.
   Matching complaints: slow runs of 5 to 10 minutes for simple tasks, and quota that runs out fast.

7. Ask only what changes the result, and state the assumptions. At most two questions. After that, one line listing the
   assumptions, for the person to check. Evidence: an agent that asks only on hard tasks solved 69.4%; one that never
   asks solved 61.2%; one given the full spec solved 70.8% (arXiv 2603.26233). Caution: after answering questions, people
   review less and accept even wrong answers (180 participants, arXiv 2611.01087). Faber already has default cards; the
   assumptions line is what is missing.

8. Team only for reading and review in parallel, one writer per area. At most about four active agents, sized to review
   capacity, and one merged status instead of one notice per agent. Evidence: Anthropic says code work is poor for teams;
   Cognition shows parallel writers make conflicting decisions; Cursor found that 20 agents produced what 2 or 3 did. A Cohere
   test found a team beating a single agent (63.2% vs 52.1%), which contradicts the others. The "about four" limit is a
   proposal, not a measured value.

## Not yet known

These contradictions need Faber's own bench before any of the build items depends on them.

- Cheap or strong planner? The Cohere test found a cheap planner nearly as good at about 70% lower cost. The same test
  found 61.8% at US$155 for a strong leader with cheap planning workers, against 63.2% at US$510 for an all-frontier team. Gemini CLI does
  the opposite: strong planner, cheap executor. Both forms need a test.
- Direction of cross-vendor review. One study says direction matters: Claude reviewing Codex helps, the reverse hurts.
  Another says a fresh session of the same model ties cross-vendor review. The mixed pair won in both.
- Teams in code. Anthropic says teams do not pay off for code; Cohere measured a gain (63.2% vs 52.1%). The task type probably
  explains the difference.
- TDD instruction vs context. Reasoning with tests helps; a generic TDD instruction hurts. Which one Faber should use is
  open.
- Best-of-N at equal budget. Most of the gain disappears when the budget is equal. It only seems to pay with an
  executable verifier.

# Coherence traps, round 2: harder traps (2026-10-16)

Second run of the R145 trap tasks (R151), after [round 1](2026-10-16-coherence-traps.md) found the eleven first traps too
easy to tell the conditions apart. Same harness (`testkit/coherence/run.mjs`), seven new traps in
`testkit/coherence/traps.json`; every scored run: [2026-10-16-coherence-traps-2.json](2026-10-16-coherence-traps-2.json).

## Result

The harder traps still barely separate the conditions: **15 of 21 plans passed with the map (71%) and 13 of 21 without
it (62%)**. Only one trap split them clearly (step up, 3/3 against 1/3); two traps failed in both conditions every time.
Over both rounds the map stands at 46/54 (85%) against 43/54 (80%), still inside the noise of three repetitions per cell.
A plan cost the same with or without the map (US$0.197 against US$0.194, about 400k input tokens, 100 s).

| Trap | Tempts | With map | Without |
| :--- | :--- | :--- | :--- |
| projects-no-ai | a Projects-only copy of Chat's "No AI" state | 3/3 | 3/3 |
| goal-attach-files | a second attachment handler for the goal box | 3/3 | 3/3 |
| mission-model-choice | a model choice outside the composer | 0/3 | 0/3 |
| goal-kept-on-restart | rebuilding a goal draft store that already exists | 3/3 | 3/3 |
| game-quiet | a "gaming mode" for what "Play a sound" does | 0/3 | 0/3 |
| retry-thinking-harder | a new retry button beside step up | 3/3 | 1/3 |
| mission-needs-in-window | a new signal beside the amber count | 3/3 | 3/3 |

## What the plans did

- **retry-thinking-harder**, the one clear gap: with the map every plan widened step up ("Try again one step up") and
  kept its words; without it, two of three renamed it ("Think harder", "Try a deeper answer"). The map's Words table
  names step up and its incoherence 8 says the answer's pickers are the owner's call; product-rules.md says neither.
- **mission-model-choice**, failed by all six, but differently: without the map every plan put a "Mission coding model"
  select in Settings > General (citing C011 to keep it off the work screen); with the map none used Settings (the map
  says the model choice lives in the composer) and all three put a picker in the goal box instead, two of them by
  extracting the composer's picker. The map moved the picker; it did not stop it. No plan raised the request itself,
  though every one raised the budget bounds that stronger models would need.
- **game-quiet**, failed by all six under the strict rubric: no plan added a gaming mode or a new setting, and all six
  named R92 (sound only in full screen) and asked the owner before changing it, but every one also planned the change
  to silence full screen. Under round 1's rule ("listing it fails even when it waits for the owner") that fails; under a
  reading where asking first is enough, all six would pass. Either way, no difference between conditions.
- The cross-area traps (projects-no-ai, goal-attach-files) and the already-built one (goal-kept-on-restart) passed
  everywhere: the agent found Chat's `Empty` state, the composer's `attach` and `saveGoal` by reading the code. Three
  plans without the map and two with it also moved the goal onto the shared `ui/unsent.ts` store, a duplication the map
  does not list.
- Every plan with the map cited it (21/21); no plan without it could.

## What changed from round 1

- Requests no longer name the existing thing: "tell them what to do first" (the No AI state), "a gaming mode" (Play a
  sound), "thinking harder" (step up), "nothing in the window shows it" (the amber tag). The agent found the thing
  anyway in all but the step-up case, by reading the code.
- Two traps aim at Projects reusing a Chat part (`Empty`, the composer's attachments), and one at code that shows the
  wrong pattern next to the right one (the project row's amber tag counts only conversations, known incoherence 2).
- The traps where the map's knowledge is not in product-rules.md (mission-model-choice, retry-thinking-harder) are the
  ones where the conditions behaved differently. That is where a round 3 should look.

## Rubric changes after the first scoring

Three rubrics changed after the first verdicts, and those plans were scored again blind (`--rejudge`); the JSON keeps
the first score where it changed. Before the changes the run scored 12/21 with the map and 9/21 without.

- projects-no-ai: all six plans had failed only for a what's-new entry (`notes.6.fixed`). R142 asks for one per change a
  person notices, so the rubric contradicted a decision; it now allows `notes.*`. Six FAIL to PASS.
- mission-needs-in-window: two plans failed for listing waiting missions as rows in the running column or the pick-up
  lists, which round 1's jobs-tab rubric passes. The rubric now allows rows in existing lists. Two FAIL to PASS.
- game-quiet: "changes R92 by itself without raising it with the owner" left open a plan that raises it and also plans
  it; the rubric now fails that, as round 1's link-account-label does. One PASS to FAIL, with the map.

## Method

As round 1: a fresh `codex exec` (gpt-6.1-sol, medium effort, read-only sandbox, no apps, plugins, hooks, subagents or
MCP servers) in a `git archive` copy of `phase-1-base` at 6cb2005, with and without the product map; a JSON plan; a
blind `claude -p` judge (Sonnet, low effort, no tools). Three repetitions per trap and condition: 42 plans. Every rubric
fact was checked in the code and against product-rules.md and decisions.md (R8, R74, R92, R123, R142) before the run.

Quota before the run: Codex 25% of its week; Claude 75% with about 45% of its week gone, ahead of pace, so the Claude
agent path (`--cli claude`) still has not run. Spend: US$9.19 API-equivalent (US$0.98 of it the judge, rescoring
included).

## Limits

- One agent model at one effort, as in round 1.
- Plans, not diffs. A diff mode did not fit in a few lines: each run needs its own writable copy with a git baseline,
  instead of the two shared read-only copies the harness builds now.
- One judge per plan; 9 of 42 verdicts moved, all from the rubric changes above.
- game-quiet's verdict rests on a strictness choice the rules leave open: is a plan that asks first and also plans the
  change incoherent? The owner may want to settle it once for every rubric.

## Next

- A Claude run of all eighteen traps once Claude's weekly quota is behind pace (smoke one trap first).
- Round 3 traps built only from what the map knows and product-rules.md does not: the closed "lives on" list, the
  stored-twice values (incoherence 20), the shared parts list, the right-click menu table.
- A diff mode for two traps, with one copy per run.

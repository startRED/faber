# Coherence traps, with and without the product map (2026-10-16)

First run of the R145 trap tasks (R151). Harness: `testkit/coherence/run.mjs`; traps and rubrics:
`testkit/coherence/traps.json`; every scored run: [2026-10-16-coherence-traps.json](2026-10-16-coherence-traps.json).

## Result

The map made no measurable difference on these eleven traps: **31 of 33 plans passed with the map (94%) and 30 of 33
without it (91%)**. One run in 33 is inside the noise of three repetitions per cell. The map cost about 4% more per plan
(US$0.162 against US$0.155) for the same reading (about 293k input tokens, 7.7 commands, 88 s).

| Trap | Tempts | With map | Without |
| :--- | :--- | :--- | :--- |
| sound-setting | a setting that already exists | 3/3 | 3/3 |
| auto-switch-on-home | a toggle on the Chat home (C011) | 3/3 | 3/3 |
| model-picker-under-answers | a second model picker | 3/3 | 3/3 |
| link-account-label | a new name for connecting an account | 1/3 | 1/3 |
| projects-search-box | a Projects-only copy of the sidebar search | 3/3 | 3/3 |
| mission-cap-on-board | a setting on the mission board | 3/3 | 3/3 |
| file-card-menu | a second right-click menu | 3/3 | 3/3 |
| notify-every-answer | a notification for everything that finishes | 3/3 | 3/3 |
| jobs-tab | a new word and a second surface for missions | 3/3 | 3/3 |
| recents-remove-confirm | copying a known incoherence (Recents' own menu) | 3/3 | 2/3 |
| autonomous-default-on-start-card | copying a known incoherence (settings on the start card) | 3/3 | 3/3 |

What the map did change: every run with it opened `docs/product-map.md`, and 31 of 33 plans cited it (mostly a "Known
incoherences" line) when they explained what they would raise with the owner. The plans were better argued, not more
often right.

## Why the gap is small

- Both copies keep [product-rules.md](../product-rules.md), whose placement table, taste cards and named past failures
  ("Redo with…", the three names for connecting an account) already answer most of these traps. The test isolates the
  map, by design, so this is the map's marginal value over the rules.
- The agent reads the code well: about eight searches per plan found the existing setting, search box, shared menu or
  running column on its own. Each request names the thing it touches, which makes the existing one easy to find.
- The only trap both conditions mostly fail is a wording one: four of six plans still listed "Link your AI account" as a
  new text while asking the owner about it. Knowing the incoherence did not stop them from shipping the asked-for label.

## Method

- Agent: a fresh `codex exec` (gpt-6.1-sol, medium effort, read-only sandbox, no apps, plugins, hooks or subagents;
  no run called an MCP server, and the harness now turns those off too), with the owner's usual Codex settings, in a `git archive` copy of `phase-1-base` at de73dfc in the temp folder, so
  `git` could not reach the map's history. Claude's weekly quota was ahead of pace (70% used, 44% of the week gone), so
  this run used Codex only.
- Without the map: the same copy minus `docs/product-map.md` and its test, with every pointer to them rewritten
  (`without` in traps.json); the harness fails if any file outside the dated pages still names the map. Both copies
  lose `testkit/coherence/` and these research pages, so no agent saw a rubric.
- Output: a JSON plan with a fixed schema (files, new settings, new controls, new screen texts, what it reuses, what it
  raises with the owner). Plans were scored, not prose.
- Judge: `claude -p` (Sonnet, low effort, no tools), given only the request, the rubric and the plan, never the
  condition.
- Three repetitions per trap and condition: 66 plans. Spend: US$11.69 API-equivalent (US$1.23 of it the judge,
  rescoring included), plus about US$1.20 of smoke runs.

## Rubric changes after the first scoring

Two rubrics changed after the first verdicts, and the plans were scored again blind with the final rubrics
(`--rejudge`); the JSON keeps the first score where it changed.

- file-card-menu: the first rubric asked for the menu as requested, but Open and Delete are not actions file cards have,
  and R123 lets a menu list only existing actions (the cross-vendor review caught the contradiction). The final rubric
  passes a plan that holds them for the owner. Two plans without the map had failed for exactly that; both now pass.
  All three plans with the map had raised R123 too.
- link-account-label: "listing that text fails even when it waits for the owner's answer". One plan with the map had
  passed while listing the new name; it now fails.

With the first rubrics the nine original traps scored 26/27 with the map and 23/27 without. Both changes make the
rubric match the written rules; the second hurt the map's score.

## Limits

- One agent model at one effort. A Claude run is still due, once its quota is behind pace.
- Plans, not diffs. An agent that writes the code may drift further than its plan.
- One judge per plan; 3 of 66 verdicts moved between the two scorings, all from the rubric changes above.
- The owner's global Codex instructions (written in Portuguese) loaded in both conditions; four plans came back in
  Portuguese.

## Next

Harder traps are what would separate the conditions: requests that do not name the existing thing (the setting lives
under another word), cross-area cases where Projects should reuse Chat's part, and requests where the code itself shows
the wrong pattern next to the right one. A diff mode for two or three traps would show whether plans hold once code is
written. The weekly coherence pass (R151) watches the real code meanwhile.

# Make the plan count and the marks say what they are

Area: Agent's plan and closing box
Files: ui/Plan.tsx, ui/Run.tsx, ui/Running.tsx, ui/Sidebar.tsx, ui/styles.css, ui/locales/en.ts, ui/locales/pt-BR.ts, ui/locales/es.ts, ui/locales/fr.ts, ui/locales/de.ts, tests/ui/plan.check.ts, tests/ui/running.check.ts, docs/design/conversation.md, docs/design/looks.md
Depends: none
Size: about 250 changed lines

Why: the live look of 2026-10-29 (`docs/research/2026-10-29-faber-diagnosis/README.md`, "Live look"), three marks that read
as something else.

- "Plano 1 de 4" counts finished steps while the second step is the current one. `planWords` in `ui/Plan.tsx` line 7
  (`plan.count`: "{done} of {total}") feeds the plan head, `run.block.plan` in `ui/Run.tsx` line 182, the running
  column (`ui/Running.tsx` line 192) and the sidebar tooltip (`ui/Sidebar.tsx` line 368). Say "1 de 4 feitas" ("1 of 4
  done") in all four places with one new key.
- The plan rows draw an empty box before each step (`.plan li .mark`, `ui/styles.css` line 1606: 14 px, rounded 4 px, an
  inset outline), which looks like a checkbox the person can tick. The plan is "a list of facts: nothing in it moves or
  takes input" (comment of `PlanList`). Draw a state mark instead: a check for done, the live ring for the current
  step, a small dot for waiting.
- A filled 7 px square, with a pulsing ring, marks "running" in the header pill (`.live`, `ui/styles.css` line 305,
  used in `ui/App.tsx` near line 1841), the sidebar row (`.run-dot`, line 134, `ui/Sidebar.tsx` line 473) and helper
  rows (`AgentRow`). It reads as a Stop button. `docs/design/subagents.md` line 76 already says a background task keeps
  "a ring, never the square of a stop button": make the running mark everywhere a ring or a dot with the ring, and
  keep Stop square controls only where Stop is a button (`ui/Commands.tsx`, `CommandStop`).

Leave out: the single live state (tasks 121 to 123), counts of running things (task `each-count-its-own-noun`).

Done: a screen check in `tests/ui/plan.check.ts` fails today because the count reads "1 of 4" and the rows hold an
empty box; it passes with "1 of 4 done" and a state mark per row, and `tests/ui/running.check.ts` finds no 7 px square
mark with a 1 px radius in the header pill or the sidebar. Extends: product-map "Agent's plan and closing box" and
"Running column". Update `docs/design/conversation.md` and `docs/design/looks.md` (the "running dot" line 101). What's-new
entry in every language. Reduced motion keeps the ring still (the `[data-motion='reduced']` rule near line 1338). Walk it
at 720 px, light and dark. Visual critique before and after. No memory or processor cost (CSS only). Review by the
other vendor (R146) and the coherence review.

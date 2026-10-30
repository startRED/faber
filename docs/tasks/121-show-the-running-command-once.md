# Show the running command in one place, and let a long one wrap

Area: Work log
Files: ui/Thread.tsx, ui/AgentChat.tsx, ui/styles.css, tests/ui/thread.check.ts, tests/ui/worklog.check.ts
Depends: none
Size: about 150 changed lines

Why: the live look of 2026-10-29 (`docs/research/2026-10-29-faber-diagnosis/README.md`, "Live look"): the running shell
command appears twice in the main column, once inside the opened work log step and again as the latest line under it,
whether or not the step is open. In `ui/Thread.tsx` the opened step draws the tool (`StepRow` and `ToolChip`, near line
204) and the same step is drawn again by `latest()` (line 370) in `activity-latest` (line 524), and by `after()` (line
386) under the words (line 565). Also, in the opened step a long command is cut at the edge: `.tools span` has
`white-space: nowrap` (`ui/styles.css` line 588) and `ToolChip` without `full` shortens the text with `middle()`
(`ui/AgentChat.tsx` line 82).

What to build: keep the work log disclosure's open state in a signal (`onToggle` of `<details class="activity">`, line
500). While it is open, the step it lists is the one place the running command shows: `activity-latest` and the `after`
line are not drawn for that step (the opened list marks the running step with the live mark). While it is closed, the
latest line stays as today. In the opened step, let the command wrap (`overflow-wrap: anywhere`, `white-space: normal`,
two lines then "show more" is the existing `full` pattern of the side column, `.side-body .tools span.full`, line 360).

Leave out: the new single live sentence (task `one-live-sentence` builds on this rule), what the latest line says.

Done: a screen check in `tests/ui/thread.check.ts` fails today because with the work log open the command text appears
twice in the reply; it passes with one occurrence open or closed (one in the log when open, one in the latest line when
closed) and a 200-character command wraps inside a 720 px column. Extends: product-map "Work log". Walk it at 720 px
and in the longest language. What's-new entry in every language. The commit states there is no memory or processor
cost. Review by the other vendor (R146) and the coherence review.

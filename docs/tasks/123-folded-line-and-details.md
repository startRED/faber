# Fold finished work to one line and move the counts behind "Ver detalhes"

Area: Work log
Files: ui/Thread.tsx, ui/Run.tsx, ui/Details.tsx, ui/foot.ts, ui/locales/en.ts, ui/locales/pt-BR.ts, ui/locales/es.ts, ui/locales/fr.ts, ui/locales/de.ts, tests/ui/run.check.ts, tests/ui/details.check.ts, tests/ui/thread.check.ts, docs/design/conversation.md
Depends: one-live-sentence
Size: about 400 changed lines

Why: `docs/research/2026-10-29-faber-diagnosis/README.md`, part 3 (Claude desktop: "Recuperou 1 memória, executou 10
comandos ›"; Grok and T3 Code: finished work folds to "Worked for 10m 22s ›" and opens into one headline per step with
commands grouped). Faber's finished answer still stacks: the work log summary `activity.done` ("Worked · n steps · n
commands", `ui/Thread.tsx` line 501), the run block title `run.block.title` with "plan 1 of 4" (`ui/Run.tsx` line 185), the away line (`AwayLine`, near line 910), the model and effort header, the cheaper-helpers note
(`plan.economy`), the provider-switch sentence (`reason.moved.*`) and `DetailsLine` in `ui/Details.tsx`.

What to build: when an answer is done, one folded line "Trabalhou por 15 min ›" (the elapsed work, no counts) that opens
the work log. Counts of steps and commands, raw commands, the full plan, model and effort, the away log, and the
cheaper-helper note move behind the existing "Ver detalhes" (`DetailsLine`, `showDetails` in `ui/Details.tsx`, the
Technical details pane); nothing that needs the person is ever moved there (a question, a failed check, "needs you"
stay in the thread). A provider switch or a summarized history is said once, in the line, not again in the run block.
The run block and the away line stop repeating the same night under two sets of hours and turns: the run block keeps
the turns, the away line moves into details and says what it measures (see task `keep-going-lines-say-what-they-measure`).
If `one-live-sentence` has not drawn "Ver detalhes" yet, add it here as the link under the sentence.

Leave out: the live state while working (task `one-live-sentence`), the new Technical details content (the pane exists),
Chat's lean list.

Done: screen checks fail today because the finished answer shows five separate small lines: one folded line, details
holding the counts and the away log, a question staying visible, a switch told once. Extends: product-map "Work log",
"Technical details" and "Keep going". Update `docs/design/conversation.md` and `docs/design/keep-going.md`. What's-new
entry in every language. Walk it at 720 px, longest language, reduced motion, light and dark. Visual critique before and
after. This task may reach 600 lines; if so, land the fold first and the details move second as a follow-up task
added to the queue. Review by the other vendor (R146) and the coherence review.

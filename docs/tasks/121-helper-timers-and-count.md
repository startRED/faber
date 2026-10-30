# Give each running helper its own timer and say how many have finished

Area: Subagent step
Files: ui/Thread.tsx, ui/AgentChat.tsx, ui/locales/en.ts, ui/locales/pt-BR.ts, ui/locales/es.ts, ui/locales/fr.ts, ui/locales/de.ts, tests/ui/agents.check.ts, docs/design/subagents.md
Depends: none
Size: about 250 changed lines

Why: `docs/research/2026-10-29-faber-diagnosis/README.md`, part 3, and the live look of 2026-10-29 (T3 Code: each helper is
a card with a title, a state and its own timer, and one line says "2 of 3 delegated tasks finished"; two photos of Faber
18 s apart showed nothing that moved). In Faber a helper row (`AgentRow`, `ui/Thread.tsx` near line 165) shows its state
and step count, and `agentMeta` (`ui/AgentChat.tsx` line 148) adds how long it took only after it ended (`started` and
`ended` of the `ToolUse`). While it runs there is no time at all, so a long helper looks the same as a stuck one.

What to build: in `agentMeta`, a helper that has `started` and has not ended shows how long it has run, in the same
`elapsed` words (`ui/running.ts`). The clock is the one the reply already has for long steps (`clock`, `longFor` in
`Thread.tsx` near line 405: a timer made only while a step runs and the window is shown, 15 s). Share it, do not add a
second timer per helper. Above the `.agents` list, when a reply has two or more helpers, one line "{done} of {total}
helpers finished" (plural forms by hand in every language, "ajudantes" as the product map names them). A helper that
needs the person or seems stuck keeps its amber row; the line never counts it as finished.

Leave out: the single live sentence (task `one-live-sentence`), helper titles (task 130 and PR #447 own them), the
helper's own page.

Done: a screen check in `tests/ui/agents.check.ts` fails today because a running helper shows no time: two helpers, one
running 3 min and one done, show "3 min" on the first, no time change while the window is hidden, and the line reads
"1 of 2 helpers finished". Update `docs/design/subagents.md` and the product-map "Subagent step" row; add the
what's-new entry in every language (`docs/next/<branch>.json`). Walk it at 720 px and in the longest language. Visual
critique before and after. The commit states the processor cost (no new timer). Review by the other vendor (R146) and
the coherence review.

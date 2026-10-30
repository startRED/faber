# Show one calm live sentence while an answer works

Area: Work log
Files: ui/Thread.tsx, ui/foot.ts, ui/calm.ts, ui/styles.css, ui/locales/en.ts, ui/locales/pt-BR.ts, ui/locales/es.ts, ui/locales/fr.ts, ui/locales/de.ts, tests/ui/thread.check.ts, tests/ui/foot.check.ts, docs/design/conversation.md
Depends: live-work-state, show-the-running-command-once
Size: about 400 changed lines

Why: `docs/research/2026-10-29-faber-diagnosis/README.md`, part 3, and the live look of 2026-10-29. While an answer works,
the person reads small grey footnotes: the latest step twice (`activity-latest` and `activity-latest after` in
`ui/Thread.tsx` lines 524 and 565), the foot line "Working" (`footText`, `ui/foot.ts`), the work log summary
`activity.working` and the tally. T3 Code shows one line, "Working for 26m 50s", whose time ticks every second, and under
it one live line in plain words ("Patching the file", "Thinking"). Two photos of Faber 18 s apart showed nothing changed.

What to build: where those lines draw now, draw one sentence at reading size (the answer's size, not `fs-xs`): "Trabalhando
há 15 min: <what>" from `liveWork()` (`live-work-state`), with the elapsed time visibly ticking (one 1 s timer, made only
while the reply works and the window is shown, like `tickWhile` in `ui/foot.ts`; the text node changes, not the
layout). With no plain description it says "A IA está trabalhando", never a shell line. A question or approval waiting
on the person replaces it with the question itself and its buttons (kind `needs`); a failed or stopped answer says so
once, about its own answer only. Put the wait rule in a small `ui/calm.ts`: a status text shows only after about 400 ms
and stays at least 400 ms, so it never flickers (T3's rule), and the live region (`role="status"`) announces a change of
the words, never a tick of the clock. The sentence keeps the rule of `show-the-running-command-once` (a running command shows in one place only) and its
check.

Leave out: the folded line after the work and the move of counts into details (task `folded-line-and-details`), the
helper list (`helper-timers-and-count`), any new setting, and the notice T3 Code shows when a request waits over 120 s
(stuck-step notices belong to task 110 and PR #444).

Done: screen checks fail today because the latest step shows twice and the foot says "Working": one sentence appears
only after 400 ms, a step that lasts 200 ms never shows, a changed step stays 400 ms, the live region text does not
change on a tick, and a raw command never appears. Extends: product-map "Work log" and "Running column". Update
`docs/design/conversation.md`. What's-new entry in every language. Walk it at 720 px, in the longest language, with
reduced motion, light and dark. Visual critique before and after (`visual-critique`, with the live look). The commit
states the processor cost of the 1 s timer. Review by the other vendor (R146) and the coherence review.

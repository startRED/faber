# Finish the 2026-10-17 motion and polish round

Start only after #110 (question card), #113 (resizable panes, focus mode, window drop), #118 (answer bar, viewer,
reading width) and #122 (the checks page never sees reduced motion) have landed on `phase-1-base`; until then leave
this task for the next one.

1. Since #122 the checks page always runs with full motion, so the early returns on
   `matchMedia('(prefers-reduced-motion: reduce)')` and the reduced-motion ternaries left in `tests/ui/appear.check.ts`,
   `find.check.ts`, `palette.check.ts`, `update.check.ts` and `question.check.ts` are dead: remove them so those checks
   run in CI.
2. `ui/Palette.tsx` swallows Ctrl+N, Ctrl+, Ctrl+. and Ctrl+F while open, but not Ctrl+B, so focus mode toggles behind
   the open palette. Swallow it too, with a line in the palette check.
3. In the running column an answered question card still vanishes at once: `ui/Running.tsx` unmounts it when
   `item.permissions` empties (`<Show when={item.permissions.length > 0}>` around `Asks`). Keep it mounted for
   `--dur-exit` (`keepForExit` in `ui/motion.ts`) so the card's own fold-away runs there as in the thread.
4. Still instant after #114: the tutorial's own exit (App.tsx unmounts it), the tutorial ring's size, the quota meter
   bars and the fold chevron. Animate those whose fix keeps to the transform/opacity rule; say which stay and why.

Done when: the five check files have no reduced-motion early return, Ctrl+B is swallowed by the palette with a check,
the running column's card folds away with a check, and item 4 is done or explained in the PR.

# An answer being typed in a question card survives a refresh of the running list

Area: Running
Files: ui/Running.tsx, tests/ui/question.check.ts
Depends: none
Size: about 80 changed lines

Why: commit b625895e on `queue/running-needs-you` (2026-10-23) kept the answer card a person is typing in when the
running list refreshes, by reconciling the list (`createStore` and `reconcile`) instead of replacing it. Its other half,
the stuck-helper text, landed in #430. The first half never landed: its landing was refused because the base moved, and
the branch now conflicts with the stuck row #430 added.

Build: first show the loss on phase-1-base with a check in `tests/ui/question.check.ts`: type into a question card,
refresh the running list, and the text must still be there. If it passes on the base, delete this task. Otherwise port the
reconcile part of b625895e by hand (about 15 lines in `ui/Running.tsx`), without its locale and docs parts.

Done: the new check fails on the base and passes with the change. Then delete `queue/running-needs-you`.

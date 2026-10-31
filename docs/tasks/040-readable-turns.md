# Readable turns: fold, group sentences, docked questions

From the T3 Code study (`01-output-rendering.md`). A finished turn folds to "Worked for 2m 3s" (approvals, questions and
live helper cards never fold); tool rows group into a sentence ("Ran 3 commands", "Changed 4 files") plus one live row;
detail of a row is fetched only when opened; approvals and questions dock in the composer with number-key answers;
streaming code parses only up to the last closed fence. Pure functions for grouping and folding, each with a test.
Measure memory and processor on a long turn (R125) and say so in the commit.

Inventory on the refreshed base: `ui/worklog.ts` and `ui/Thread.tsx` already group tools into sentences and keep
the current tool visible; `tests/ui/worklog.check.ts` and `tests/ui/thread.check.ts` cover these. Extend them.
`ui/Ask.tsx` and the existing permission cards remain the only answer controls: docking moves the same card, never
creates another independently answerable copy. Number keys act only on the focused card, never while typing in a
text field or composing text. Preserve free-text answers and keyboard/screen-reader access.
`ui/markdown.tsx` already caches closed streaming blocks; visual-answer checks cover unfinished fences. Measure
the actual remaining parse cost before replacing this path. Fetch-on-open needs a measured payload problem, not
another cache by default. Preserve approvals, questions and live helpers when folding completed work.

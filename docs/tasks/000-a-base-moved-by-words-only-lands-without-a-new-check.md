# A base that moved only by words lands a green queued commit without checking it again

Area: none
Files: scripts/land.mjs, scripts/land.test.mjs, .github/workflows/ci.yml, docs/dev/workflow.md
Depends: none
Size: about 150 changed lines; the ci.yml part (plain queue branches) may follow as a second pull request

Why: from 11:00 to 13:00 on 2026-10-30, one pull request landed while seven sat in the queue. Each of them ran three or four
times (for example `queue/pr-470`: runs at 11:32, 11:57, 12:20 and 13:01, every earlier one cancelled), because every
landing moves phase-1-base and the land job then sends every queued commit back to be squashed and checked again
(`landQueued` in `scripts/land.mjs`: `if (!ancestor(base, sha)) return alone(...)`; for plain `queue/<name>` branches, the
rebase and `gh workflow run` at the end of the land job in `ci.yml`). Five of the six moves of the base in those two hours
were commits of task files only (`docs/tasks/*.md`), which no suite reads (`scripts/ci-scope.mjs`: `isWords`). With runs of
about 20 minutes and a landing every 15, almost nothing reaches the end of a run.

Build: when the base moved, compare the tree the queue run checked with the tree that would land (the commit squashed or
rebased again on the new base). When every file that differs between them is words by `isWords` (the same rule CI uses to
skip every suite), and `node scripts/docs.mjs` passes on the new tree, land it at once instead of sending it back. Anything
else (a file of code, a conflict, a numbering that moved a locale line) is checked again as today. Keep "one landing at a
time". In `docs/dev/workflow.md` (Merge on green), say what is landed without a new run, and why that is safe.

Done: a test in `scripts/land.test.mjs` for the decision (words only, so it lands; one code file, so it is checked again;
a locale line, so it is checked again). One real case shown in a run log: a pull request queued, a task file landed under
it meanwhile, and the pull request landed without a second run. A review by the other vendor: it is the path every
change takes onto the base.

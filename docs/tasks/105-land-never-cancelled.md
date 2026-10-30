# A landing that waits for another is never dropped

Area: none
Files: .github/workflows/ci.yml, scripts/land.mjs, scripts/land.test.mjs, docs/dev/workflow.md
Depends: none
Size: about 150 changed lines

Start only after PR #446 has landed or closed (it changes ci.yml); until then, `node scripts/tasks.mjs release
land-never-cancelled` and take another task.

Why: the `land` job of `.github/workflows/ci.yml` runs in `concurrency: group: land` with no queue. GitHub keeps one
running and one pending job per group and cancels the older pending one, so when three queue runs turn green together the
middle one's landing is cancelled: its gate is green, nothing lands, the `report` job is skipped, the pull request's
`queue` check stays pending and nothing wakes its session (seen on queue/pr-450, 2026-10-30; `scripts/pace.mjs` counted 75
cancelled queue rounds in 7 days). The lead session re-runs those by hand today (`gh run rerun <id> --failed`).

Build: a landing never lost. Either the land job, after landing its own head, lands every other queue head whose gate is
green and is not on the base yet (one sweep, oldest first, the same `land.mjs --land` path), or a cancelled land re-queues
itself; pick the smaller one that is correct when the base moves meanwhile. The `report` job also runs when `land` was
cancelled, so a pull request never sits with a pending `queue` check.

Done: a test in `scripts/land.test.mjs` (the sweep picks green, unlanded queue heads in order and skips red or landed ones)
that fails today; workflow.md's "Merge on green" says what happens to a landing that waited. One review by the other vendor
(it is the path every change takes onto the base).

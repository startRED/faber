# Landing work

## Merge on green

`gate` (R173) is a job in `ci.yml` that is green only when `changes`, `check`, `app` and `screen` passed or were left
out on purpose by the plan. `main` and the queue (R162) run after it, so the queue only pushes a commit whose gate
passed. GitHub no longer requires it on `phase-1-base` (R186): a required check made GitHub refuse the land job's own
push even with the gate green ("Required status check gate is expected", queue/pr-74 on 2026-10-17), and the queue
already lets in only checked commits.

Nobody waits on CI (R182). Right after opening a pull request, its lane runs `node scripts/land.mjs <number>`
([Numbers](workflow.md#numbers)) and goes on to its next task. `land.mjs` puts a pending `queue` status on the pull request's head.
When the queued commit's gate or its landing fails, the `report` job turns that status red and comments on the pull
request with the run, which wakes the session that watches it (the app's auto-fix). The fix is pushed to the pull
request and queued again with `land.mjs`; a push alone never lands, since the landing stops at a head it did not squash.
`land.mjs` is the only way in (R186). A pull request's own run checks only `changes`, the title and the lints, since
the queue checks the very commit that lands; the suites ran twice per change before, once on the pull request and
once on its squash or its queue commit, and five landings at once left screen parts waiting up to 226 s for a runner.
Before retrying an old stacked pull request, compare its intended changes with landed descendants. Squashing a
descendant can incorporate every change without closing the ancestor's pull request. If all implementation, checks,
notes and documentation are already present, close the obsolete request as incorporated instead of restoring its old
tree. Keep the landing and comparison evidence; a remaining code or research change still needs its own review.

So a pull request is never merged with the button or `gh pr merge`: its green says only that the lints passed. A
Dependabot pull request still runs every suite on its own, until it lands through `land.mjs` too. Each push to a queue
branch cancels the run before it, so a lane queues once, with its fix complete.

Batches (R190 V3, R220). A pull request that touches other parts of the repository than one queued and not landed
yet (its first folders, leaving out the words, the locales and `docs/`, which every landing writes) is squashed and
numbered on top of that queue commit instead of the base, up to three deep and never with a workflow change. One run
then checks them together, and when it is green the land job (`land.mjs --land`) moves every branch of the batch to
its own commit and phase-1-base to the top one, so all of them show merged; the runs under it find themselves landed.
Before selecting a queued batch, `land.mjs` skips it if any pull request under it is already closed, retargeted, pushed
again, requeued, or red; the landing job checks again after the suites to catch changes made during the run.
One under it that was queued again, closed, pushed again or turned red, or a base that moved, sends the top one back to
be squashed alone. A red batch blames nobody yet: the report job (`land.mjs --split`) queues the top one again alone,
and each side then answers for itself, so the one whose own run is red is the culprit and gets the red `queue` status.
`--alone` queues without batching.

The PC checks before it queues (R190 V2). `land.mjs` runs, on the very commit it is about to queue, the suites CI would
run for the change (`cargo t --suite <name>`, each whole), in one worktree it keeps for that
(`faber-wt-landing` beside the main checkout, marked as its own and building into `target/landing`, one landing at a
time, on Windows only, where the screen suite runs). When they pass, it
posts the commit status `faber/verify` with the owner's own `gh` sign-in, saying the tree and the suites
(`tree <hash> covers rust ui screen`), and only then pushes the queue branch. The `changes` job takes that status only
from the repository's owner, only on a queue branch and only for the tree it names, and leaves those suites out: the
lints still run, and a tree without it runs as before. So a lane runs `land.mjs` in the background (the suites take
minutes). A failure there queues nothing and prints the failing tests; `--no-verify`, a run on CI (the land job's
run again on a new base), no `cargo` on the PATH or another landing checking at the time queue without the status,
and CI runs every suite. The status is only as trusted as the owner's PC: any program there can post it with the
owner's sign-in, as the spec accepts, and the daily run on `main` catches a false green within a day. Every suite still runs on `main` once a day and before a commit becomes stable, which is also
where macOS runs its Rust suite now (Erick, 2026-10-17): the PC proves macOS through Clippy only (R166).


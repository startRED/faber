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
`land.mjs` is the only way in (R186). Internal human pull requests keep the title check and leave CI to the queue,
which checks the exact commit that lands. Fork and Dependabot pull requests keep their independent checks.
Waiting queue riders run only planning and train dispatch; their train checks the documents and every required suite.
Before retrying an old stacked pull request, compare its intended changes with landed descendants. Squashing a
descendant can incorporate every change without closing the ancestor's pull request. If all implementation, checks,
notes and documentation are already present, close the obsolete request as incorporated instead of restoring its old
tree. Keep the landing and comparison evidence; a remaining code or research change still needs its own review.

So a pull request is never merged with the button or `gh pr merge`: its green does not prove the queued commit. A
Dependabot pull request still runs every suite on its own, until it lands through `land.mjs` too. Each push to a queue
branch cancels the run before it, so a lane queues once, with its fix complete.

Trains (R497, replacing the batches of R220). The queue checks one train at a time. A queued pull request's own run
tests nothing (`changes` says `waits`); its `train` job sends a train when none runs. `land.mjs --train` takes the
queued pull requests with the lowest numbers (one queued again keeps its place) still at the head that was squashed, at
most three, squashes and numbers each on the one
before it, moves each `queue/pr-<n>` there, points the branch `train` at the top one and starts one run on it. Green,
the land job (`--land`) moves every rider's branch to its own commit and phase-1-base to the top, so all show merged,
and sends the next train. Pull requests queued meanwhile wait instead of cancelling anything: with one run each, every
landing sent all the others back to the start (2026-11-01: of 38 queue runs, 1 green, 6 red, 31 cancelled). A red
train blames nobody yet: `--split` sends its first half again and keeps its second half next on the branch
`train-then`, before any newcomer, and a red rider alone is the culprit, whose red `queue`
status keeps it off trains until it is queued again. A rider pushed again or closed, or a base that moved, sends the
next train with whoever stands. A cancelled run reports nothing. `queue.yml` sends a train every twenty minutes when
none runs. A workflow change never rides: it is checked alone and landed with `--finish`.
When its checks pass, the queue status asks for `--finish` and keeps the checked commit instead of splitting and checking
it again; `--finish` still refuses an incompatible base or a changed pull request.
If GitHub never acquired a runner, the gate stays red but the report retries the train once instead of splitting or
blaming riders. Another capacity failure holds the train with an infrastructure summary; it never reports a code failure.
Before the costly app builds and native suites, an Ubuntu preflight checks the combined tree's documentation and Rust
formatting. A failure stops those jobs and leaves the gate red; the existing checks still run after a passing preflight.
On run 41950963030, a formatting error had already failed while the gate waited another ten minutes for the other jobs.

The PC checks before it queues (R190 V2). `land.mjs` runs, on the very commit it is about to queue, the suites CI would
run for the change (`cargo t --suite <name>`, each whole), in one worktree it keeps for that
(`faber-wt-landing` beside the main checkout, marked as its own and building into `target/landing`, one landing at a
time, on Windows only, where the screen suite runs). First it runs the cheap checks the change needs: formatting and Clippy for Rust, lint and typecheck for the screen, and the docs check. CI still runs its own. When they pass, it
posts the commit status `faber/verify` with the owner's own `gh` sign-in, saying the tree and the suites
(`tree <hash> covers rust ui screen`), and only then pushes the queue branch. The `changes` job takes that status only
from the repository's owner, only on a queue branch and only for the tree it names, and leaves those suites out: the
lints still run, and a tree without it runs as before. So a lane runs `land.mjs` in the background (the suites take
minutes). A failure there queues nothing and prints the failing tests; `--no-verify`, a run on CI (the land job's
run again on a new base), no `cargo` on the PATH or another landing checking at the time queue without the status,
and CI runs every suite. The status is only as trusted as the owner's PC: any program there can post it with the
owner's sign-in, as the spec accepts, and the daily run on `main` catches a false green within a day. Every suite still runs on `main` once a day and before a commit becomes stable, which is also
where macOS also runs its native suites. Trains and standalone queue checks run the scoped macOS Rust and UI suites
inside the existing Mac lints job, sharing its runner and cache. A Windows PC's proof never skips these native suites.
The existing Ubuntu documents job checks Linux engine and runner compilation whenever the queue needs lints.

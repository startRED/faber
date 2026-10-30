# Workflow

How work lands: lane claims, merge on green, the task queue, the direction check, the red base and the numbers given at landing.

## Lane claims

Lanes that run at once (R128) say first what they will change (R154). A worktree on a branch writes `.faber-lane` at its
root before it touches a file; git ignores the file:

```text
area: Settings screen
own: ui/Settings.tsx
own: engine/src/settings/
share: ui/locales/
```

`area` is a row or heading of [product-map.md](../product-map.md), or `none` for work no person sees. `own` is the lane's
alone; `share` is a file several lanes may append to, such as the locales or the decisions. A path claims itself and
everything under it. Before any suite, `cargo t` (`testkit/src/lanes.rs`) stops when a changed file is not claimed, when
an area is not in the map, or when a claim overlaps another worktree's (two `share` claims may). The main checkout (the
lead session, CI) and detached worktrees (Faber's own missions, test workspaces) need no claim. A lane deletes its claim
once its last commit is on `phase-1-base`; a claim nobody saved for a day is stale and ignored. Claims live on one PC:
a lane on another machine is not seen.

## Merge on green

`gate` (R173) is a job in `ci.yml` that is green only when `changes`, `check`, `app` and `screen` passed or were left
out on purpose by the plan. `main` and the queue (R162) run after it, so the queue only pushes a commit whose gate
passed. GitHub no longer requires it on `phase-1-base` (R186): a required check made GitHub refuse the land job's own
push even with the gate green ("Required status check gate is expected", queue/pr-74 on 2026-10-17), and the queue
already lets in only checked commits.

Nobody waits on CI (R182). Right after opening a pull request, its lane runs `node scripts/land.mjs <number>`
([Numbers](#numbers)) and goes on to its next task. `land.mjs` puts a pending `queue` status on the pull request's head.
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

Batches (R190 V3, R220, R481). A pull request is squashed and numbered on top of a queue commit not landed yet
instead of the base, up to three deep and never with a workflow change, whatever parts both touch: checked side by side
on the base, the first to pass would land and send the other back for a whole new run. One run
then checks them together, and when it is green the land job (`land.mjs --land`) moves every branch of the batch to
its own commit and phase-1-base to the top one, so all of them show merged; the runs under it find themselves landed.
Before selecting a queued batch, `land.mjs` skips it if any pull request under it is already closed, retargeted, pushed
again, requeued, or red; the landing job checks again after the suites to catch changes made during the run.
One under it that was queued again, closed, pushed again or turned red sends the top one back to be squashed alone; a
base that moved (other than by words only, below) sends it back to join a batch on the new base the same way, so the
ones sent back by one landing are checked together instead of one run each. A red batch blames nobody yet: the report job (`land.mjs --split`) queues the top one again alone,
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

## Task queue

A session that finished a task takes the next one by itself, without asking, the way it does when the owner says "I am
going to sleep, keep going" (R185). The list is `docs/tasks/`: one file per task, `<order>-<name>.md`, lowest order
first, with a `# Title` line and the brief a fresh session needs. One file each means adding, reordering or finishing a
task never touches another's lines. The owner decides what goes there; a session may add a follow-up it found after
the owner's tasks.

`node scripts/tasks.mjs next` reads the list on `origin/phase-1-base` and claims the first free task by creating the
branch `claim/<name>` on GitHub. A push without force fails when that branch is already there, so two sessions never
get the same task, whatever PC they run on. The claim is a commit of its own whose message says who holds it and since
when; one older than a day is stale, and the next session takes it over (the same day as lane claims). The session
works on a branch named after the task, and its pull request deletes the task's file, so the task is gone once it lands;
the next `next` deletes the claims of tasks that left the list. A task whose `Depends:` line names a task still on
the list waits: `next` skips it. `list` shows who holds what and what waits, `release <name>` gives a task back. CI never runs on `claim/` branches.

A task is the size of one pull request: one idea, about 100 to 400 changed lines. A task that needs more than about
1000 is split into tasks that land one after another, each one a working step on its own. Its file names the area, the
files, what done looks like and the tasks it depends on. Many small pull requests a day keep their quality the same way
one does: the local proofs, the queue and a review by the other vendor.

New task files go in through a pull request and `land.mjs`, which batches them on top of the pull requests already queued,
never as a plain push to `queue/<name>`: each landing moves the base, and today every queued commit is then checked
again from the start (task 000 changes that for moves of words only).

A session asks the owner only what is the owner's to decide (direction, taste, money), in one short card, and takes the
next task while the question waits. It stops when `next` says no task is free.

## Direction check

Rules become tests and the queue keeps sessions busy, but nothing else asks whether the work still heads where
[docs/product/](../product/README.md) says (R243). `node scripts/direction.mjs` asks a fresh read-only Codex session
(`gpt-6.1-sol`, `codex exec -s read-only`, nothing kept) to read the product pages, the product map with its "Known
incoherences", the decisions and every queued task, and answer three questions: which goals have no feature yet, which
features drift from their goal, and which queued tasks no longer serve one. Every finding names the page or decision it
rests on, and the report says what it could not judge.

It writes `docs/tasks/proposed/REPORT.md` and one proposed task per gap in the task format, each marked as a proposal;
git ignores the folder, and the queue never reads it. It queues nothing: the owner or the lead session picks a proposal
by giving it an order and moving it into `docs/tasks/`, and drops a task the report flags only after checking it. A new
run replaces its own proposals and keeps any other draft there. `--prompt` prints the question without running Codex.

The lead session runs it once a week, and any session runs it before stopping when `tasks.mjs next` says no task is
free. A run reads about 300 KB of pages, so it costs one long Codex turn of quota and nothing on the PC.

## Red base

A commit pushed onto a red `phase-1-base` waits behind the failure, and `main` cannot follow it (R127); on 2026-10-16 a
macOS-only check kept it red for three hours while ten commits landed on top. So the red base shows before a push
(R164). `scripts/base.mjs` asks GitHub through `gh` for the newest finished push run of `ci.yml` on `phase-1-base`
(cancelled runs say nothing) and, when it failed, reads the runner's `FAIL` lines and compiler errors from its failed log,
each with its job, or the failed jobs and steps when the log names none. It keeps the answer three minutes in the shared
git folder (`.git/faber-base.json`). Every `cargo t` outside CI prints those lines first, marked `BASE`, and writes the
`pre-push` hook (`testkit/src/base.rs`) unless the person has a hook of their own. The hook refuses a push to
`phase-1-base` while it is red, or one that carries a number placeholder; `FABER_PUSH_ON_RED=1 git push` pushes the fix,
and a push to any other branch only prints the warning. Offline, signed out, or without Node or `gh`, it says nothing and
refuses nothing: it is a warning light, not a gate.

Local proofs run on Windows, so a check that holds only there turns the base red on macOS. A check that measures text or
pixels measures the glyphs (`Range.getBoundingClientRect()`), never the box that holds them, waits for
`document.fonts.ready`, and keeps a margin wide enough for the other system's font rendering; the bound and the worst
value seen on Windows go in its comment.

## Owed work

A lane fixes what it queued before it starts something new (R466). On 2026-10-28 one lane had six pull requests whose
queue runs had failed since 2026-10-25 (a macOS compile error, a broken test, lints) and kept building new work on top
of them for three days, so none of it could land; the red `queue` status had woken it, and nothing held it. Each
`land.mjs` run on a PC records the pull request in the worktree's own git folder (`faber-queued.json`, beside its
`HEAD`). A recorded pull request that is still open is owed while its `queue` status is red, or missing because it was
pushed after queueing and not queued again; one that landed or was closed is forgotten. While one is owed, `cargo t`
prints it, `node scripts/tasks.mjs next` gives no task, `land.mjs` queues no other pull request, and the `pre-push`
hook refuses a push of any branch but the owed pull requests' own (deleting a branch always goes through).
`FABER_IGNORE_OWED=1` goes past it for work an owed pull request waits on. Like the red base, offline or without `gh`
it refuses nothing (`scripts/owed.mjs`).

## Numbers

Decision numbers (`docs/decisions.md`) and what's-new ids (`notes.<id>.<kind>`) are given when a change lands, never in
a branch (R164, R179): lanes that each took "the next one" collided at every merge, and lanes that each appended to the
end of the decisions table and of the five locales' entries conflicted at every merge. On 2026-10-16 and 17 that loop
(merge the base in, wait for CI, find the base moved again) cost 590 session-minutes, more than any other step.

So a branch never touches those shared lines. Its entry and its decision rows go in a file of its own,
`docs/next/<branch>.json` (a branch named `a/b` writes `a-b.json`):

```json
{
  "notes": { "better": { "en": "…", "pt-BR": "…", "es": "…", "fr": "…", "de": "…" } },
  "decisions": ["| <placeholder> | 2026-10-17 | The decision. | AI |"]
}
```

`notes` holds an entry per kind it names, each with an id of its own and a text in every language of `ui/locales/`,
each written by hand. A row of `decisions` is written like the table's, with `R-next` for its number. Elsewhere the
branch writes `R-next` for its decision (`R-next` for a second) in every place that refers to it, the pull request
title included. Quoted in backticks the placeholders are only words, as here. Two branches then conflict only where they
change the same lines for real.

`node scripts/land.mjs <pull request>` lands it: it squashes the pull request onto the newest `phase-1-base` with
`git merge-tree` (the working tree is never touched) and names the commit after the title and number. In a throwaway
worktree, `node scripts/numbers.mjs assign --amend` folds the fragment in after the last entry of each locale and the
last row of the table, and every placeholder, the commit message's too, takes the next free number after that base. The
numbered commit goes to `queue/pr-<number>`, where the queue checks that very commit (R162), so the base and the
required `gate` (R173) only ever see checked commits. Its land job, one landing at a time, moves the pull
request's branch to the landed commit, from the head that was squashed only (the commit's `Pull-request-head` line),
and then `phase-1-base`, so GitHub shows the pull request merged (one atomic push of both closed #64 instead); a base
that refuses puts the branch back; a later push, a close or another target stops the
landing, and `land.mjs` replaces a queued commit only when nothing newer was queued meanwhile. Two decision rows that
share a placeholder stop the numbering. When `phase-1-base` moved during the checks, the land job runs
`land.mjs --head` again on the new base instead of a rebase, whose numbers would collide, and the queue checks it again.
One move is the exception: when the base moved by words only (no suite reads them, the rule CI skips every suite by;
`docs/decisions.md` and `docs/next/` left out, since numbers are taken there), the green batch is put on the new base as it
is and lands at once, provided every file that then differs from the tree CI checked is words too and the docs check
passes. A task file landing meanwhile no longer sends every queued pull request back to the start.
GitHub refuses the workflow's own token any push of a commit that changes a file in `.github/workflows`, so such a
landing ends on the PC: once its gate is green, the land job fails with that reason, and `land.mjs <number> --finish`
makes the same atomic push with the person's own sign-in, after checking the gate passed, the commit is still on top of
`phase-1-base` and the pull request is still at the squashed head. `--dry-run` shows the commit it would queue and pushes nothing. A fragment that cannot fold stops `land.mjs` with its
reasons, and `scripts/numbers.test.mjs` checks every waiting fragment wherever the lints run.
It also fails on a decision number used twice, and on any placeholder or fragment left when CI runs on `phase-1-base` or
`main`; the `pre-push` hook refuses the same before it leaves the PC. The older form, `notes.next.<kind>` written
straight into the locales and `node scripts/numbers.mjs assign` run on the branch, still works.

A pull request never lands with `gh pr merge` (R173 is amended): GitHub would squash the placeholders onto
`phase-1-base` unnumbered, and the base's check would fail.

The shared docs merge themselves too (R184). Two lanes that each add a row, a line or a paragraph at the same place of
`docs/product-map.md`, a `docs/design/` page or any other `.md` file conflict in plain git, though both additions belong. So
`land.mjs` squashes with `merge.conflictStyle=diff3` and settles each conflict in which neither side changed a line the
base had: the base's new lines first, then the pull request's (`insertions` in `scripts/land.mjs`). When one complete
added block is already contained in the other's whole lines, it is kept once with the surrounding additions intact.
Anything else stops
the landing with its files: a conflict in code, a file one side deleted, or a line both sides changed, which is a real
disagreement for the lane to settle. The squash is checked like any other, `engine/tests/product_map.rs` included. A
conflict that GitHub shows on a pull request in those files alone needs nothing: the landing settles it.

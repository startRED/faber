# Workflow

How work lands: lane claims, merge on green, the task queue, the process freeze, the direction check, the red base and the numbers given at landing.

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

[Merge on green](landing.md#merge-on-green) explains the queue, batches and local checks.

## Task queue

A session that finished a task takes the next one by itself, without asking, the way it does when the owner says "I am
going to sleep, keep going" (R185). The list is `docs/tasks/`: one file per task, `<order>-<name>.md`, lowest order
first, with a `# Title` line and the brief a fresh session needs. One file each means adding, reordering or finishing a
task never touches another's lines. The owner decides what goes there, directly or through the session he names; the list holds
at most 15 tasks (R489), and a session that finds a follow-up tells that session instead of writing a task file.

`node scripts/tasks.mjs next` reads the list on `origin/phase-1-base` and claims the first free task by creating the
branch `claim/<name>` on GitHub. A push without force fails when that branch is already there, so two sessions never
get the same task, whatever PC they run on. The claim is a commit of its own whose message says who holds it and since
when; one older than a day is stale, and the next session takes it over (the same day as lane claims). The session
works on a branch named after the task, and its pull request deletes the task's file, so the task is gone once it lands;
the next `next` deletes the claims of tasks that left the list. A task whose `Depends:` line names a task still on
the list waits: `next` skips it. `list` shows who holds what and what waits, `release <name>` gives a task back. CI never runs on `claim/` branches. `node scripts/pace.mjs` prints, for the last day and week, the pull requests landed with their size and time to land, the queue rounds green, red and cancelled with the three checks that failed most by chance (`flaky: <check> ×<n>`, a check that failed and later passed on the same pull request head), and the free tasks.

A task is the size of one pull request: the biggest coherent slice one AI does well, one feature or fix with its tests,
often 300 to 1,000 changed lines (R490). Each pull request costs a queue run, so a string of 100-line steps of one idea
costs more than one change. A task that needs much more than 1,000 lines is split into tasks that land one after another,
each one a working step on its own. Its file names the area, the
files, what done looks like and the tasks it depends on. Many small pull requests a day keep their quality the same way
one does: the local proofs, the queue and a review by the other vendor.

New task files go in through a pull request and `land.mjs`, which batches them on top of the pull requests already queued,
never as a plain push to `queue/<name>`: each landing moves the base, and today every queued commit is then checked
again from the start (task 000 changes that for moves of words only).

A session asks the owner only what is the owner's to decide (direction, taste, money), in one short card, and takes the
next task while the question waits. It stops when `next` says no task is free.

## Process freeze

Until 2026-11-14 (R489) the process machinery changes only to fix a red base or a landing that is blocked:
`scripts/land.mjs`, `scripts/tasks.mjs`, `scripts/pace.mjs`, `testkit/` and `.github/`. The W9 audit
(`reports/reforma/workflow-audit.md` outside the repository) found 38% of the commits since 10-24 spent on that machinery,
task files and CI, and 124 task files written in one day while 122 waited. A change it does need names the red run or the
blocked pull request in its body.

## Board

[BOARD.md on the branch `board`](https://github.com/startRED/faber/blob/board/BOARD.md) shows in one page what is being
done (fresh claims, with who and for how long), what is landing (queue branches), what comes next (free tasks, in
order), what waits on another task and the last ten landings. `node scripts/tasks.mjs board` builds it from the same
state `next` reads, and nobody writes it by hand. `next` and `release` publish it after a claim or a release, and
the land job publishes it after each landing of a pull request (`land.mjs --land`). Each list stops at ten rows, so the page stays a few KB long however long
the queue grows. It is the only file of a branch that holds one commit with no parent: publishing replaces it whole and
never conflicts with anything, and CI never runs on that branch. Sessions read `next` and `list`, not the board.

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
(R164). `scripts/base.mjs` asks GitHub through `gh` for the newest finished push or dispatched run of `ci.yml` on `phase-1-base`
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
A green check is not thrown away when the base moved elsewhere (W9 audit, 2026-10-31: 43% of queue runs were green
checks cancelled by their own re-queue, and `queue/pr-470` looped 23 times in 11 hours). When no file the batch changes
was changed on the base meanwhile, its squash applies cleanly on the new base, its own files come out byte for byte as
checked, no workflow is involved, and numbers taken on the base meet none the batch took, the land job puts it on the
base as it is, then starts one full check of `phase-1-base` (`workflow_dispatch`) instead of moving `main`; the `main`
job moves `main` and starts the preview release once that check is green. While `main` waits behind the base for
such a check, every landing starts one too instead of moving `main`, since a later batch's own check may not cover the
whole tree. Pending checks of the base replace each
other, so one run covers every landing made meanwhile. Two changes that are each green but break together show as a red
base (`scripts/base.mjs` reads these runs too) and never reach `main`. Another exception: when the base moved by words only (no suite reads them, the rule CI skips every suite by;
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

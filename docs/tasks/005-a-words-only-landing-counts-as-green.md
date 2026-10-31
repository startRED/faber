# A commit landed on a base moved by words counts as green for the next run's scope

Area: none
Files: scripts/land.mjs, scripts/land.test.mjs, .github/workflows/ci.yml, docs/dev/workflow.md
Depends: a-base-moved-by-words-only-lands-without-a-new-check
Size: about 150 changed lines; the plain queue branch part may follow as a second pull request

Why: task 000 (#480) lands a green batch on a base that moved only by words without a new run. The commits it lands
are cherry-picked again, so their own SHAs never had a run. `green()` in `.github/workflows/ci.yml` (the `decide` step)
calls a commit green only when a run checked that very SHA, so the next queued commit built on one of them gets
"Everything runs: <base>, the commit this one builds on, is not green (yet)" and runs every suite: the saving of #480
comes back as a full run one landing later.

Build:

1. When `landQueued` (`scripts/land.mjs`) lands through the words-only path, mark each landed commit with what was
   checked: a commit status `faber/checked-as` (state success, description = the SHA of the queued commit whose run
   passed), posted by the same sign-in the land job already uses.
2. `green()` accepts a commit that has that status when the named commit had a green run and the two trees differ only
   in words (`isWords` of `scripts/ci-scope.mjs`; the land job already checked it, so reading the status is enough if
   only the land job can post it: say in a comment why).
3. Second part (may be its own pull request): the same words-only shortcut for plain `queue/<name>` branches at the
   end of the land job in `ci.yml` (`git rebase` then `gh workflow run`): when the base moved only by words and the
   rebase is clean, land the rebased commit at once instead of starting another run.

Done: a test in `scripts/land.test.mjs` for the status the words path posts; one run log where a queued commit built on
a words-only landing shows a scoped run instead of "Everything runs"; a paragraph in `docs/dev/workflow.md` (Merge on
green); one review by the other vendor (this is the path every change takes onto the base).

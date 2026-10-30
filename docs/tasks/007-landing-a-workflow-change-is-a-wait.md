# A landing that must finish on the PC is a wait with the right next step, not a red round

Area: none
Files: .github/workflows/ci.yml, scripts/land.mjs, scripts/land.test.mjs, docs/dev/workflow.md
Depends: land-never-cancelled
Size: about 100 changed lines

Why: of the 134 failed rounds on `queue/*` in the 7 days up to 2026-10-30, 18 failed in the `land` job, and 14 of those
18 are one designed case that looks like a fault: the queued commit changes a file in `.github/workflows`, and GitHub
refuses the workflow's token that push, so the `land` step ends with `exit 1` ("changes a workflow, which this token may
not push", `ci.yml` near line 475). In 4 of them the gate was green and the message was right ("land it with node
scripts/land.mjs <n> --finish"; runs 40982345234, 40983532565, 40263952623, 40191033536); in the other 10 the base had moved
meanwhile and the message says "queue it again from a PC" (runs 40948414067, 40777869619, 40710721151, 40207887319, 40207721931,
40194106787, 40191021800, 40023820144, 40007758520, 39997527060). In both cases the `report` job then posts a failed `queue`
status and a comment that says "Queued, but it could not land on phase-1-base. Fix, push, then: node scripts/land.mjs <n>":
the wrong instruction for a green gate, a red mark on the pull request, and a wake of the lane's session for something
that is not a failure. The rest of the 18 are honest: 3 conflicts with the base (queue/pr-454 in
tests/ui/visual-answers.check.ts, pr-399 in docs/dev/benchmark.md, pr-337 in testkit/src/screen/sentinel.rs) and 1 pull
request closed while queued.

Build: wait for `land-never-cancelled` to land first (it changes the same `land` job). Then split the case in the land
step: when the commit changes a workflow, the step ends green after posting the `queue` status as `pending` with the
description "Checks passed: finish it with node scripts/land.mjs <n> --finish" (when the base moved, the description says
`land.mjs <n>` queues it again, then `--finish`), and `report` is skipped for it. Check what the `main` job and the `gate` need
from `land` so a green `land` that landed nothing does not start anything it should not. Make `land.mjs --finish` print the
same sentence and accept the pull request in the state `pending`. Update the "Merge on green" paragraph in
`docs/dev/workflow.md` that says "the land job fails with that reason". A test in `scripts/land.test.mjs` for the sentence
chosen for each case (gate green, base moved).

Done: a queue run of a pull request that changes only a workflow ends with the `queue` status pending and the finish
command, not failure, shown by the test and by one real queued pull request that changes a comment in `ci.yml`; workflow.md
says so. One review by the other vendor (it is the path every change takes onto the base).

# A stuck CI test job ends after a time limit instead of holding a runner

Area: none
Files: .github/workflows/ci.yml
Depends: landing-a-workflow-change-is-a-wait
Size: about 5 changed lines

Why: the branch `queue/ci-job-timeouts` (commit on 2026-10-28) gives four CI test jobs a `timeout-minutes` (45 and 60), so a
hung test build ends instead of holding a Windows runner for GitHub's default six hours. Its gate was green; it never
landed only because a commit that changes a workflow must be finished from a PC (`land.mjs <n> --finish`) and the base
moved first. `ci.yml` on phase-1-base still has no `timeout-minutes`, and `git merge-tree` with the base is clean.

Build: open a pull request from that commit (`git cherry-pick origin/queue/ci-job-timeouts`), check that the limits still
sit well above each job's longest run of the last week (`gh run list` and the jobs' durations; the screen parts run about
3 min, the rust job about 10), and land it the way task 007 leaves workflow changes. Then delete `queue/ci-job-timeouts`.

Done: phase-1-base's `ci.yml` has the limits, each one at least three times the job's longest run in the last week, and the
old queue branch is gone.

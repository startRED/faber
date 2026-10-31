# A green landing is never dropped while it waits for another

Area: none
Files: .github/workflows/ci.yml, scripts/land.mjs, docs/dev/workflow.md
Depends: land-never-cancelled
Size: about 60 changed lines

Why: the `land` job has `concurrency: group: land`, and GitHub keeps only one waiting job per group: a third landing that
starts while one runs and one waits cancels the waiting one. On 2026-10-30 between 17:00 and 18:40 UTC nothing landed
although several queue runs had a green gate (for example run 41100869014: gate success at 18:31, land cancelled two
seconds after it started). `land.mjs --sweep` (#471, #474) lands such a run again afterwards, one at a time, but every
landing still waits for runners twice and a burst of green runs keeps cancelling each other.

The group is not what keeps the base right: `landQueued` pushes `phase-1-base` without force, so of two landings on the
same base only one push is accepted, and the other goes back (`alone`, or onto a batch once #482 is in) after `back()`
restores the pull request branches it moved.

Build: drop the shared group from the `land` job (or give each queue branch its own group, so a newer push of the same
branch still replaces its older landing), and make sure two landings at once stay safe: the pull request branch moves
and `back()` (a landing that lost the push restores only what it moved), `main` fast-forward (non-force, so the loser's
push fails harmlessly: check `landQueued` does not treat that as an error), and the preview release dispatch (once per
landed commit). Then `--sweep` at the end of each landing remains only for a run cancelled by other means. This changes a
workflow file: it lands with `land.mjs <n> --finish` from the PC once its gate is green.

Done: a test in `scripts/land.test.mjs` for whatever ordering `landQueued` needs when its base push loses; a paragraph in
`docs/dev/workflow.md` (Merge on green) replacing the one about the `land` group; one queue run log where two landings ran
at the same time and one went back; one review by the other vendor (this is the path every change takes onto the base).

# The sales proof test holds on a slow macOS runner

Area: none
Files: scripts/bench/practical.test.mjs, bench/proofs/sales.mjs
Depends: none
Size: about 30 changed lines

Why: `sales proof rejects a hardcoded fixture answer on fresh input` (`scripts/bench/practical.test.mjs:102`) failed in
`check (macos-latest, lints)` of `queue/pr-462` run 41071854644 (2026-10-30 14:32, 22.7 s), a pull request that does not
touch `bench/` or `scripts/bench/`. The test runs the proof twice; each proof
(`bench/proofs/sales.mjs`) starts `node summarize.mjs` ten times, each bounded by its own 5 s `timeout`, while the test
bounds the whole proof at 15 s (`proof()` in `workspace`). On a loaded macOS runner ten Node starts pass 15 s, the outer
`spawnSync` kills the proof, and `result.status` is null, so the test fails although the proof would have answered.

Build: let the inner limits bound a hang and the outer one only a proof that never ends: either raise the outer `timeout`
of `proof()` above the sum of the inner ones (ten starts at 5 s), or start the proof's `summarize` runs in parallel with
`spawn` and keep one limit per run. Keep the "must exit nonzero, not hang" checks: a summarize that hangs must still fail
the proof, not the test. When the outer limit fires, the assertion message says so (signal and elapsed time) instead of
an empty `stderr`.

Done: `npm run test:scripts` passes; a self-check (a summarize that sleeps past its 5 s limit) still makes the proof
answer false within the outer limit; the pull request names the red run above.

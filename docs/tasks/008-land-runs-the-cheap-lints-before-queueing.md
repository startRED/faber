# The PC runs Clippy and the format check before it queues, so a lint error never costs a queue round

Area: none
Files: scripts/land.mjs, scripts/ci-scope.mjs, scripts/land.test.mjs, docs/dev/workflow.md
Depends: none
Size: about 90 changed lines

Why: of the 134 failed rounds on `queue/*` in the 7 days up to 2026-10-30, 14 had a failing `check (windows-latest, lints)`
job: 13 are Clippy errors (`-D warnings`, `items_after_test_module`, `too_many_arguments`, `cloned_ref_to_slice_refs`, a
type error in `engine/tests/stuck.rs`) and 1 is `cargo fmt` (queue/pr-383 run 40978180177), plus 3 runs where `cargo test
--workspace --no-run` did not compile. The newest three are after the PC check of R190 V2 existed: pr-444 run 40975292272
(2026-10-29 23:45), pr-383 run 40978180177 (2026-10-30 00:12) and pr-444 run 41010734019 (05:51). Cause:
`toCheck` in `scripts/ci-scope.mjs` returns only `rust`, `ui` and `screen` ("the lints aside (CI runs them)"), so `land.mjs`
runs the tests on the PC, queues, and the lints run for the first time in CI; each such error costs a full queue round, the
red mark, a wake, a fix and a second round (about 20 minutes and a whole run of runner time), and a pull request batched
under it (R190 V3) is sent back with it. Both checks are cheap where the code is: `cargo fmt --all --check` takes seconds and
`cargo t --suite lint` is the same proof CI runs (testing.md, "Clippy proofs").

Build: in `check()` of `scripts/land.mjs`, before the suites, run `cargo fmt --all --check` and `npm run lint` when a file of
their kind changed (use `suitesFor` / `isWords` to tell), and `cargo t --suite lint` when a Rust file changed. Measure first on
the landing worktree (warm `target/landing`) how long `cargo t --suite lint` takes and write it in the pull request: if it is over
about three minutes, run only the Windows Clippy proof (`--workspace --all-targets`) and leave the macOS one to CI. These steps
are checks only: the `faber/verify` status still names `rust ui screen`, and CI still runs the lints (`covered()` accepts
nothing else), so no trust is added. A failure prints the lines and queues nothing, like a failing suite. Update the "The PC
checks before it queues" paragraph of `docs/dev/workflow.md`. A test in `scripts/land.test.mjs` for which commands a given
list of changed files starts (Rust file: fmt and lint suite; words only: none).

Done: the test above passes; `node scripts/land.mjs <n>` on a pull request with a deliberate Clippy warning queues nothing and
prints which warning. One review by the other vendor.

# The verifier's full check confirms on GitHub (R190 V2)

Read `docs/superpowers/specs/2026-10-17-built-in-verifier.md` (section "GitHub: confirmation, not discovery" and step
V2) and the verifier V1 in the engine (`verify` in the product map). `scripts/land.mjs` asks the PC's verifier for a full
check on the exact tree it squashed and, when it passes, posts the commit status `faber/verify` with the owner's own
`gh` sign-in (never a CLI credential). The `changes` job of `.github/workflows/ci.yml` compares the tree hash (not the
commit id) and leaves out the suites the status covers; the lints still run. macOS runs once a day and before each
release (owner, 2026-10-17). The daily full run on `main` reports every test the day's selections missed.

Done when: a queued run whose tree has a green `faber/verify` runs only the lints and the gate stays green; a tree
without it runs as today; `scripts/land.test.mjs` and a ci-scope test cover both; workflow.md and testing.md say how.

# The rest of verifier V1

R203 built the verifier without these parts of V1 (`docs/superpowers/specs/2026-10-17-built-in-verifier.md`, sections
4 and 5): a change feed from a file watcher with a `git status` rescan on overflow, the graph from `cargo metadata` and
the TypeScript imports (the same rules as `cargo t` and `scripts/ci-scope.mjs`), admission by free memory instead of
one heavy run at a time, the game guard (no new run and a CPU cap while a full-screen app is in front), and the results
journal (failed and flaky first, run-cost estimates). Each part keeps idle cost at zero processes and timers.

Done when: each part has its engine test and the leak checks of section 5 that apply; the product map's Checks row and
the spec's step list say what V1 now covers.

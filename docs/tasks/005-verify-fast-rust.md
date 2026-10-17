# The verifier's fast level checks a Rust change in seconds

The verifier (R203) runs the project's Checks, which for Faber are the proof commands of AGENTS.md. None of them
compiles Rust (`cargo t --claims` builds only the runner), so a fast check of an engine change runs `cargo fmt` and
nothing that would catch a compile error, and the agent learns it only from CI. Give the fast level a quick Rust check
of the crates a change touches (`cargo clippy -p <crate> --all-targets -- -D warnings`, and that crate's unit tests when
they take seconds), in the same job, priority and output rules as the other checks, without adding it to the full level
or changing what AGENTS.md lists. V0 measured a one-file check at about 7 s.

Done when: a fast verify of a separate copy with a compile error in `engine/` fails within about 30 s on a warm copy and
names the file and line; one without Rust changes runs no cargo; engine tests with the fake runner cover the selection;
the product map's Checks row says what the fast level adds.

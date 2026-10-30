# The recorded CLI fixtures are checked per CLI version

Area: none
Files: engine/tests/cli_versions.rs, engine/tests/all.rs
Depends: none
Size: about 125 changed lines

Why: `queue/cli-recorded-versions` (2026-10-23) adds `engine/tests/cli_versions.rs` (123 lines), a test that the recorded
parser evidence in `tests/fixtures/` names the CLI version it was recorded with and that the parsers read each one, plus an
opt-in workflow. The workflow now overlaps the nightly `cli-qualification.yml`; the test does not exist on the base.

Build: take only the test (`git checkout origin/queue/cli-recorded-versions -- engine/tests/cli_versions.rs` and its line
in `all.rs`), fit it to today's fixtures, and leave the workflow and its dated plan page out. If
`cli-qualification.yml` already checks the same thing, delete this task instead.

Done: `node scripts/cargo.mjs t --filter cli_versions` passes, and fails when a fixture's version line is removed. Then
delete `queue/cli-recorded-versions`.

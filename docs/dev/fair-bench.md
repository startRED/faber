# The fair benchmark against TL-ADE

How Faber's own test system is measured against TL-ADE's, for the floor of R113 (5x less time and processes, 3x less
memory and CPU). The goal and what landed are in [testing](testing.md#fabers-own-test-system), item 6.

- The meter is `faber-bench` (`testkit/src/bin/faber-bench.rs`, Windows only). It runs one command in a fresh job object
  (started suspended, joined, resumed) below normal priority, with no window, no shell and its output thrown away (a log
  would count as its writes). It reads the job's own accounting once the whole tree has ended, or 30 s after the
  command did (it then ends what is left and the line counts it as `leaked`; the wall time then holds that wait): wall time, processes started, processor time, peak committed memory and
  bytes written. It appends one JSON line with the folder's commit and the time. Before starting it waits until there
  has been no input for `--idle` seconds (60), no full-screen app is in front and no Cargo build (`cargo`, `rustc`,
  `link`) runs anywhere on the PC. `--unfair` skips the wait and marks the line, for checking the meter itself.
- `node scripts/bench/run.mjs [--only suites|matched|real-work] [--runs 3]` runs both sides through it. TL-ADE is the folder
  beside the repository, or `--tlade` (needed from a worktree). The sides are `vitest run` against `cargo t --all`, and the 8 matched scenarios
  of the spec, Faber one test module at a time (`--filter`), summed, which can only favour TL-ADE. Each gets one warm-up run, kept apart, so compilation is never measured
  (R115), then three runs. Every run goes to `docs/research/bench/<date>.jsonl`, and the medians print as a table.
- The real-work tier is 7 tasks through each app's screen with the CLI's answers replayed
  ([plan](../superpowers/plans/2026-10-22-real-work-bench.md)). Faber's are screen scenarios named "real work N", in the
  suite only when `--filter` names them. TL-ADE's driver (`scripts/bench/tlade.mjs`) starts its panel and a headless
  Chrome on a temporary repo and home, with only the stand-in CLI (`faber-standin`, as `claude.exe`) on its PATH. The
  times each side noted go to `<date>.notes.jsonl`.
- Still open: the report against the floor.

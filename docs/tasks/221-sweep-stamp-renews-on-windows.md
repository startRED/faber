# Build storage maintenance sweeps at most once an hour on Windows too

Area: none
Files: testkit/src/sweep.rs
Depends: none
Size: about 30 changed lines

Coordinate with task 220 (`recover-orphaned-nested-build-reservations`), which lists the same file: if it is claimed,
agree with its author first, and change only `due()` and its test, never the reservation or eviction code.

Why: `due()` in `testkit/src/sweep.rs` marks a sweep as done with `File::create(stamp).is_ok()`. On Windows,
re-creating an existing empty file leaves its modified time unchanged, so once the stamp is older than an hour every
call of `scripts/cargo.mjs` runs the whole sweep instead of one an hour. Reproduced in isolation on 2026-10-31 (a
`rustc`-built program in a disposable folder, the same call 3 s apart: `create_ok=true`, `changed=false`). On the
owner's PC the real stamps were dated 2026-10-18 (`target/.faber-sweep`) and 2026-10-23 (`%TEMP%/faber-target`), while
every check run that day printed "Build storage: ... exceeds the 12 GB budget", which is printed only after `due()`
said yes. That every real call swept is inferred from those two facts, not observed.

What to build: `due()` sets the stamp's modified time to `now` itself (`File::set_modified`, or the platform's
equivalent) and reports a sweep due only when that succeeded. No new stamp, setting, timer or process.

Done: a unit test in `testkit/src/sweep.rs`, in a temporary folder, that calls `due()` on an empty stamp older than an
hour, then again at once, and expects true then false; it fails on Windows before the change. The second call must not
sweep. No test touches the real `target/` or `%TEMP%/faber-target` stamps, and nobody changes those stamps by hand to
investigate. Cost: one metadata write per sweep, fewer sweeps.

# Crash lines are told apart by a counter, so a clock set back or two crashes in one millisecond lose none

Area: Projects (crash watch)
Files: engine/src/crash.rs, engine/src/project/goals.rs, engine/src/engine/goals.rs, engine/tests/goals.rs
Depends: none
Size: about 200 changed lines

Why: `engine/src/project/goals.rs:95` says two lines written in the very same millisecond lose the second, and a clock set back hides lines until it catches up: "Upgrade when a crash file carries a counter." The goal that fixes the project's own crashes then misses a crash. `crashes.jsonl` is R421's file.

Build: each crash line gets an `n` that only grows (found by reading the last line when the process starts). `CrashWatch.since` becomes the last `n` seen, with the old time field as a fallback for lines without `n`. The file's "drop the oldest half" rule keeps the counter going, since the last line stays.

Done: engine tests that fail today: two lines with the same time, the first seen, the second still counts as new; a clock set back one hour does not hide a new line; an old file without `n` still works. Remove the shortcut comment. Cost: none.

# Faber answers within seconds of opening, without waiting for the "go back" history to load

Area: Rewind
Files: engine/src/engine/rewind.rs, engine-projects/src/rewind/store.rs, engine/src/engine.rs, engine/tests/rewind.rs, engine/examples/, docs/design/rewind.md, docs/product-map.md
Depends: none
Size: about 500 changed lines

Why: plan item A1 (reports/update-grande-2026-11-25, outside the repo). The owner's `startup.log` shows the host ready
after 12 to 32 s, and almost all of it is between the `ledger` and `rewind` marks of `Engine::new`
(`engine/src/engine.rs`, `crate::startup::mark("rewind")` right after `rewind::State::new`). `State::new`
(`engine/src/engine/rewind.rs`) calls `History::open` (`engine-projects/src/rewind/store.rs`), which on the start path
reads every point and journal (`load`), recovers an interrupted cleanup, inspects project cleanup and runs a full
`cleanup` whose `refresh_usage` opens and measures every owned file (fingerprint and zlib header of each object). The
owner's `rewind` folder: 801 MB, 481 files, 123 JSON. `host/src/lib.rs` returns from `start()` only when the engine can
answer, so the window sits on its first frame for that long.

Build:

- Measure first: a small example (`engine/examples/`) that opens `History` on a copy of a real `rewind` folder and prints
  each stage (load, recover, inspect, refresh_usage, clean). Put the stage times in the commit body; the fix targets the
  slow stage, not a guess.
- Take the slow part off the start path: the engine answers before the full measurement and cleanup end. History work
  that needs the budget waits for it (a capture before it is ready waits or is skipped the way an unavailable history is
  today, never silently loses a point). Cleanup and usage run on a background thread at background priority
  (`faber_winproc::background_thread`), once, after start.
- Keep recovery first: when `pending_recovery` finds unfinished journals, admission stays closed until recovery ends,
  exactly as today (`recovery_failed`, `retry_rewind_recovery`). Only the no-pending case gets faster.
- Keep the `startup.log` marks honest: `rewind` marks when the engine can answer; add a `rewind-ready` mark (or the
  existing `Work` timings) when the background part ends, so a slow disk still shows.

Leave out: the window and its failsafe timer (task window-shows-on-first-frame); changing limits, retention or the store
format.

Done: an engine test with a history whose open is made slow (a test hook or a large fixture) shows the engine answering
before the history is ready, and fails on today's code; the existing `engine/tests/rewind.rs`, `rewind_cost` and the
recovery tests stay green, including one where a pending journal still holds the first request. Before and after
numbers on the owner's copied folder in the commit body (memory and processor cost of the background thread too).
Extends: `docs/design/rewind.md` and the product-map rows `Rewind` and `rewind/<folder-id>/`. Engine state and stored
data: review by a fresh session of the other vendor (R146).

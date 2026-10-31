# Let the store write a conversation snapshot away from the engine, never an older one over a newer one

Area: none
Files: engine/src/store.rs, engine/tests/failed_writes.rs, engine/tests/store_summaries.rs
Depends: none
Size: about 250 changed lines

Why: follow-up of PR #445 (helper activity saves a conversation at most every two seconds), and
`docs/research/2026-10-29-faber-diagnosis/README.md`, part 1. Every save serializes and writes the whole record while the
caller holds the engine lock: `Core::save_checked` (`engine/src/engine.rs` near line 1867) calls
`Store::save_checked_with` (`engine/src/store.rs` near line 391), which serializes with `write_with` and replaces the file
through `replace_with` (temp file, rename, a busy-rename wait on Windows). On the owner's 102 MB conversation that is
about 0.28 s to serialize and 0.51 s to write and flush. To move the write out of the lock the store must first be safe
to call from two threads: today nothing stops an older snapshot, written late, from landing over a newer one.

What to build, in the store only: `Store::save_snapshot(id, generation, bytes)` that takes the serialized record and a
per-conversation generation number, holds a per-conversation write lock (a small map from id to a mutex, cleaned when the
conversation is removed), skips the write when a generation at or above this one has already been written, and otherwise
runs the same steps as `save_checked_with` after serialization: the pending group-move recovery, `replace_with`,
`written` (the summary cache) and `search.saved`. `Store::snapshot(conversation) -> (bytes)` is the serialization half.
`save_checked_with` is rewritten as `snapshot` plus `save_snapshot` with the next generation, so every existing caller
keeps its behaviour, its `Fault` and its patient/impatient rename. No engine change in this task.

Leave out: the engine's use of it (task `save-outside-the-engine-lock`), compacting the file (task
`write-conversations-compact`), any change of the failure keys.

Done: tests fail today (the API does not exist) and show: a generation 5 snapshot written after generation 6 does not
land (checked by the file's content, with a `Disk` fake from `engine/tests/failed_writes.rs` that delays the first
write), two writers of the same conversation never interleave a temp file, a failed write leaves the older file and
reports the same `Fault`, and every existing save, crash-window and failed-write test still passes. Extends: product-map
"Other stored state" (no change of files). The commit states the memory cost: the serialized bytes live until written.
Review by a fresh session of the other vendor (R146), because it touches data on disk.

# Write a conversation's record outside the engine lock

Area: none
Files: engine/src/engine.rs, engine/src/engine/evict.rs, engine/tests/transcript_save.rs, engine/tests/failed_writes.rs
Depends: store-keeps-the-newest-snapshot
Size: about 350 changed lines

Why: follow-up of #445 (helper activity saves a conversation at most every two seconds). The 2 s timer in
`engine/src/engine.rs` (`save_soon` / `save_dirty`, once PR #445 has landed: check `gh pr view 445`, and read that
code first) and every other `save` still serialize and write the whole record while holding the core lock: on a 100 MB
conversation that is about 0.8 s (0.28 s to serialize, 0.51 s to write and flush) during which no command, event or
screen call is answered (`Engine::with` locks `core` for the call, `engine/src/engine.rs` line 763). Now it happens once
per two seconds at most while helpers work, instead of once per event.

Do: under the lock take the snapshot half (`Store::snapshot`, with its generation, from task
`store-keeps-the-newest-snapshot`), then write it with `save_snapshot` after the lock is released, for the saves that
nobody waits on: `Core::save` / `save_with` (`engine.rs` lines 1820 to 1832) and the timer's `save_dirty`. Saves that
acknowledge something keep the synchronous path inside the lock, because they promise durability: the callers of
`try_save` (lines 3143, 3204, 3445, 3561, 4484 and `siblings.rs` 303 and 322: a message is accepted only after it is
saved), the quit flush in `shutdown` (line 1408), `evict.rs` line 36 (a conversation leaves memory only once saved) and
the rewind saves (`rewind.rs` lines 275 and 455). Keep the dirty mark until the written generation covers it; keep save
failures, their keys and `SaveFailed` exactly as they are (the `first` logic of `save_result`, line 1840). A failure
after the lock is gone is reported the next time the engine is entered, never silently dropped. Measure first (lock hold
per save on a 20 MB and a 100 MB record; `measure_500_helper_events_on_a_20_mb_conversation` in
`engine/tests/transcript_save.rs` is a start; `big-conversation-fixture` gives the 100 MB case if it has landed) and
state the memory cost of what the snapshot holds (serialized bytes against a cloned record: serialize under the lock
if cloning 100 MB is worse).

Leave out: changing what is saved or when (coalescing is #445), the acknowledging paths, compacting the file.

Done: lock hold per save drops by the write time (numbers before and after in the commit body); a test proves an older
snapshot never lands after a newer one through the engine (a `Disk` fake that delays the first write); a test shows a
call into the engine is answered while a slow write is in flight; every existing save, crash-window and failed-write
test still passes. Extends: product-map "Other stored state". The commit states the memory and processor cost (R125)
and names the CI run that measured it. Review by a fresh session of the other vendor (R146), because it touches
the engine lock and data on disk.

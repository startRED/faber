# Write a conversation's record outside the engine lock

Follow-up of #445 (helper activity saves a conversation at most every two seconds). The 2 s timer in `engine/src/engine.rs`
(`save_soon` / `save_dirty`) and every other `save` still serialize and write the whole record while holding the core lock:
on a 100 MB conversation that is about 0.8 s (0.28 s to serialize, 0.51 s to write and flush) during which no command,
event or screen call is answered. Now it happens once per two seconds at most while helpers work, instead of once per event.

Do: under the lock take what the write needs and a per-conversation generation number; write the temp file and rename it
outside the lock; never let an older snapshot overwrite a newer one (a per-conversation write lock plus "skip if a newer
generation was written"); keep the dirty mark until the written generation covers it; keep save failures, their keys and
`SaveFailed` exactly as they are. Measure first (lock hold time per save on a 20 MB and a 100 MB record, the
`measure_500_helper_events_on_a_20_mb_conversation` test in `engine/tests/transcript_save.rs` is a start), and state the
memory cost of whatever the snapshot holds. Done: lock hold per save drops by the write time, a test proves an older
snapshot never lands after a newer one, and every existing save, crash-window and failed-write test still passes.

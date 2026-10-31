# Read the quota again as soon as the PC wakes from sleep

Area: Accounts and limits
Files: engine/src/engine/quota_watch.rs, engine/src/budget.rs, engine/src/ledger.rs, engine/tests/quota_live.rs, docs/product-map.md
Depends: none
Size: about 150 changed lines

Why: a laptop on Windows sleeps through the night. Faber's quota bars, the pace marks and the "someone else is using the account" reading are computed from the last reading, so after a wake they can be hours old and still look current, and a window reset can pass unseen.

Build: in `quota_tick` detect a clock gap (the wall clock moved far more than the monotonic tick, or the tick came much later than its period) and treat it as a wake: drop the cached readings as stale, read each connected vendor's quota at once (one read at a time, as now), and keep the foreign-use reading from judging the first new sample against the old one. No power-event API, so it works on every system and in tests.

Leave out: any shell or tray change, and the reading itself.

Done: an engine test with a fake clock that fails today: after a simulated 8-hour gap a reading starts at the next tick and the first jump is not counted as foreign use. Extends the map's quota rows. No new timer or process.

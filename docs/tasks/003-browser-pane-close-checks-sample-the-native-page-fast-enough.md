# Browser pane close checks take too few samples of the native page on a busy PC

Area: none
Files: testkit/src/screen/browser_pane.rs
Depends: none
Size: about 100 changed lines

Why: with another `cargo t` running, two checks of `testkit/src/screen/browser_pane.rs` fail on almost every local run
(2026-10-30, after the quick probe of `test_site_probe` landed):

1. "closing during a view-width transition keeps the page visible without reversing travel": "fewer than two visible
   departing samples". The close now lands during the transition, but an interrupted close lasts about 100 ms, and each
   sample waits for one `test_site_probe` answer, which takes about 65 ms on a busy PC even in quick mode (its native
   getters wait for the shell's main thread, which is placing the page every frame). So only one or two samples are taken.
2. "native navigation and placement within the memory budget": "the native page jumped instead of following close
   motion". Same sampler, same cost: the native position seen stays at its start and then reads parked.

Build: take the column's place on every frame without waiting for the probe, and still bracket each native answer by the
column's place before and after it, so that a late answer never hides a frame where the page disappeared or the column
went back (a review found that simply filling samples in later drops exactly those frames). Wait until every probe that
was sent has answered, with a time limit, before reading the trace. Never raise the 15 s.

Done: `cargo t --filter "browser pane"` passes 20 times in a row locally with another `cargo t` running; one review by
the other vendor.

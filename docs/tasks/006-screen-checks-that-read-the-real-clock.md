# Screen checks that decide on real elapsed time judge the page's own timestamps instead

Area: none
Files: testkit/src/screen/shell.rs, testkit/src/screen/sentinel.rs, testkit/src/screen/review.rs
Depends: none
Size: about 150 changed lines

Why: three screen checks pass or fail on how long the runner took between two steps of the check, not on what the page
did. A runner that stalls for a second fails them with no fault in the product.

1. Column that waits for the typing (the product waits `QUIET_MS = 1500` after the last key, `ui/running.ts` line 81). Two
   checks press a key every 0.3 s from the test process and assert that the column is not open while keys come:
   "the column waits for every key, also those that change no text, and opens with the caret where it was" failed in
   queue/pr-302 run 39963884188 (2026-10-23 13:45, "the column opened while keys were still coming",
   `shell.rs:816`); "a column opened and closed while it waits for the typing stays closed" failed in pr-442 run
   40974274721 (2026-10-29 23:36, "the column opened before the user did", `shell.rs:939`). When the CDP round trips of the
   test take more than 1.5 s between two keys (the runner stalls), the column legitimately opens and the check calls it a fault.
2. Frames in the middle of an animation. "sentinel: the native select picker opens and dismisses through real frames" failed in
   pr-362 run 40191258266 (2026-10-25 12:00, "the full-motion picker must have a real intermediate opacity",
   `sentinel.rs:345`), and 14 seconds later, in another run, "review 12: natural column motion has intermediate frames
   without stretching text" failed in pr-313 run 40191270354 (its message then was "the width travelled in less than 120
   ms"; the code was reworked since). Both read `requestAnimationFrame` samples inside a 150 to 250 ms window: if the
   renderer paints no frame in that window there is no intermediate sample, whatever the CSS does. Two failures in the same
   minute on two pull requests look like one stall of the runner. `natural_motion` also keeps `p95 < 26.0` ms as a frame
   budget; it did not fail this week, but it is the same kind of gate on a shared machine.

Build: (1) In the two typing checks, record the time inside the page: a `keydown` listener pushes `performance.now()`, a
`MutationObserver` stores the time `.runcol` mounted, and the check asserts `mounted - lastKey >= QUIET_MS - 50` (the column
may open only after the quiet time, measured by the page's own clock) instead of "not visible right now". The step that
needs the column to open by itself waits for it with the existing wait. (2) For the picker and the column motion, drive the
animation instead of sampling it: take the running `Animation` from `getAnimations()`, `pause()` it, set `currentTime` to half
its duration and read the computed opacity or width; assert the animation exists, its duration is the token's, and the
midpoint value is strictly between the two ends. Keep the real-frame sampling only as extra evidence in the notes, not as a
verdict. The `p95` budget stays but only fails when the frame count is large enough to be a measurement (say at least 20
frames); with fewer, the check notes "no frame clock" as the picker check does (`infrastructure: no picker frame clock`).
Never loosen a number to make the check pass.

Done: `cargo t --filter "the column waits for every key"`, `--filter "a column opened and closed"`, `--filter "native select
picker"` and `--filter "natural column motion"` each pass 20 times in a row locally, with another `cargo t` running to load
the PC; a temporary change that opens the column after 500 ms of quiet still fails (1) and a CSS change that removes the
picker's fade still fails (2). One review by the other vendor.

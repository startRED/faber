# The visual answers memory check bounds the growth of each reopening, not a first reading against a fixed margin

Area: none
Files: testkit/src/screen/app.rs
Depends: none
Size: about 60 changed lines

Why: the screen check "visual answers: one open frame stays within the memory budget while switching versions and after
closing" failed in 8 runs on 8 different pull requests between 2026-10-24 and 2026-10-26 (queue/catalog-manifest-native
40064272806, pr-316 40075737439, pr-332 40117770952, pr-336 40137265115, frontend-design-pane-exit 40139253462, pr-341
40139682824, pr-352 40165982920, pr-411 40258740188) (only pr-352 is about visual answers). Every time the message is the
same shape: "visual answer after Escape at 720 px: 107.0 MB, baseline 101.5 MB, retained difference 5.5 MB after 15.6s;
remaining frame references: []; exceeds the 5 MB post-close margin". The differences are 5.0 to 5.5 MB every time, the weak
references are all collected and no extra WebView2 process is left, so the DOM does let go; what remains is the working set of
the WebView2 processes after the first reopening at 720 px, most likely a one-time cost (caches, a warmed frame process; to be confirmed by the 20 runs below) that sits just above the
hand-picked margin: `margin = 5.0.min(baseline + 25.0 - previous_closed)` in the cycle loop of `testkit/src/screen/app.rs`
(near line 1614), judged by `visual_frames_collected`, which waits 15 s for the number to fall and then fails. The readings are
steady at 5.0 to 5.5 MB against a 5.0 MB line, which looks like a margin set just under the real figure, not noise.

Build: keep every exact signal (weak references, extra process ids, DOM counters) as hard checks. For the megabytes, judge
what the comment above the loop says the margin is for, retention per switch: cycle 1 only has to stay inside the existing 25 MB
cap over the cold reading, and cycle 2 must not grow more than the margin over cycle 1's closed reading (a leak grows every
cycle; a one-time cost does not). Before changing numbers, run the check 20 times and write the cycle 1 and cycle 2 readings
(the notes already print them) in the pull request; choose the cycle-2 margin from them with the reason, and do not just raise
the 5 MB.

Done: `cargo t --filter "visual answers: one open frame"` passes 20 times in a row locally; a temporary change that keeps one
frame alive after Escape (or leaks 10 MB per reopening) still fails it. One review by the other vendor.

# Browser pane checks that follow a window resize stop acting before the page has settled

Area: none
Files: testkit/src/screen/browser_pane.rs, src-tauri/src/pane/mod.rs (only if the product is at fault)
Depends: none
Size: about 120 changed lines

Why: two checks of `testkit/src/screen/browser_pane.rs` failed in 6 runs on 6 different pull requests (2026-10-25 and
2026-10-26), none of them about the browser pane's resizing: both act on the page right after the window size changed.

1. "browser pane: closing during a view-width transition keeps the page visible without reversing travel": queue/pr-397
   run 40242626699, pr-403 run 40249394862, c590-address-resize run 40268940553. All say "timed out after 15 s waiting for
   the browser to close in an observed intermediate width frame" with `preparation.events: []`, that is, the shell never
   ran a width transition. The record in pr-403 shows why: the click on the browser button happened at t=3746 ms, 367 ms
   after the listener was installed at 3379 ms (the 220 ms transition of the first click was over), the shell went to its
   new width (`widthInput` 420px to 979px), but the `browser-click-frame` record that the click handler schedules with
   `requestAnimationFrame` never appeared in the 15 s: the page produced no animation frame after that click, so no
   `transitionrun` could fire. Either the window stops painting for a while after the native child is placed (a product
   fault, to read in `pane/mod.rs`), or the check arms its observer in a gap where the frame clock is not running. The
   check waits for one single event of a 220 ms animation, seen through CDP round trips (`interrupt_width`, lines 33 to 215).
2. "browser pane: German opening failure photograph": pr-374 run 40190807891, pr-376 run 40199226758, c590-clean-copy-sweep
   run 40266841086. The last one shows the address field empty ("opening state: address ''") and the alert "Hier öffnen sich
   nur Webseiten" (the empty-address refusal) instead of the injected failure: in the trace the field was replaced by the
   resize (`field 1` to `field 2`), then `Enter` was pressed in field 2 with `value: ""` and no `input` event was ever seen.
   `failure_in` waits for the `resize` event and then calls `enter`, but the column keeps rendering after that event, so
   the typed address is lost. The two earlier runs fail one step later with "timed out after 15 s waiting for the child
   WebView2 to show one", the same race on the retry.

Build: first read `src-tauri/src/pane/mod.rs` and the shell's resize code for the two questions the logs leave open: does
the page really stop producing frames after the child WebView2 is placed in the first check, and does a person typing an
address while the window is resized lose it in the second? A yes to either is a product bug: fix it there, with the check as
its proof. If both are the checks' fault: in (1) arm the observer, then wait for the frame clock (two `requestAnimationFrame`
turns) before the click, and fail with "no frame after the click" instead of the 15 s wait when it does not run; in (2) wait
until the address field is the same element on two frames in a row and its width matches the new window before `enter`.
Never raise the 15 s.

Done: `cargo t --filter "browser pane"` passes 20 times in a row locally, with the PC under load (another `cargo t` running);
the pull request says which of the two it was, product or check, per case. One review by the other vendor.

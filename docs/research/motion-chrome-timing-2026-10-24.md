# Recoverable errors and update-entry motion

Status: locally checked and independently reviewed; integration and the full task 997 slow walkthrough are unfinished.
This slice builds on the shared timing tokens in PR 317, without changing their values.

## Two regressions reproduced

The shared card-duration override was followed by another `.ask .note.err` animation shorthand with equal specificity.
It restored the 400 ms travel duration in Dark Room and Workshop. The later shorthand now uses the short interface
duration, matching the intended override. Workshop update entries used 240 ms plus a 72 ms stagger, exceeding the
300 ms chrome limit. Their stagger now uses the existing 60 ms shared ceiling.

Two separate native checks inspect every factory theme and full/reduced/none. Before correction they failed with
400 ms for the error and 312 ms for the update entries (0 passed, 2 failed; session 77215). After correction both
passed (session 66822; 288 s total including a full-screen-app hold, 29.3 s build, 2.2 s UI). These run durations do
not measure an app speed improvement. Reduced and no motion retain the existing disabled paths.

The seven lightweight proof commands passed; source lint and the frontend build retain their existing non-failing
warnings. A fresh read-only `gpt-6.1-sol` review found no blocker in cascade, total timing, disabled paths, the checks,
the product map, design rules or five handwritten language entries. Cards checked: C001, C007, C008, C011.

Production changes two CSS declarations. No new DOM, listener, timer, dependency or process is introduced. Runtime
memory and processor cost were not measured locally; the integrated memory budget remains required.

## Real update-screen capture

The corrected hidden native app opened the actual What's New screen in Workshop at 720x480 and 1280x800. For the
before state only, a new-document stylesheet restored the old stagger; the after state reloaded without that plant.
The same corrected executable was used on both sides, not an executable of the base revision. Native receipts
assert the actual theme, the presence of the notes, the plant's presence/absence, and 240+72=312 ms before versus
240+60=300 ms after. Playback was set to 0.25x; the four images show final states, not frame-by-frame motion. The
driver's sandbox audit and cleanup passed.

Visual critique: before 39/40, after 39/40, by independent read-only `/root/review_motion_frequent`. At 720x480 the
eight rubric scores were 5,5,5,5,5,5,5,4; at 1280x800 they were all 5. One fit point was lost in both small captures:
the "Melhor" heading overlaps the Faber title-bar mark. It is present before and after, so this is not a timing-change
regression. The later [window-frame correction](notes-window-frame-2026-10-24.md) records reproduction of the viewport
starting behind the titlebar, its correction and independent 40/40 after critique. Integration remains unfinished;
that note also records the missing capture-time executable digest. No question-error image was captured or scored.
The complete inventory and remaining observations are in `motion-inventory-2026-10-24.md`.

Negative/positive logs, native receipts, screenshot pairs, capture source and corrected source/executable identities
are preserved at `E:/Documentos/ProjetosIA/faber-evidence/motion-chrome-timing-20261024/`. The capture is a controlled
regression plant, not an independent runtime approval of every animation or every question state.

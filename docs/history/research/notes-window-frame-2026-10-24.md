# Update notes below the window controls

Status: locally checked and independently reviewed. Integration is unfinished.

The small-window capture in the preceding motion slice exposed the update heading passing under the Faber mark.
The actual titlebar occupies the first 36 px of the app, but the fixed notes viewport started at 0 underneath it.
The viewport now starts at 36 px, keeping scrolling entries below the window controls on every platform.

The new native check builds the actual app grid and titlebar and compares their boundary with the notes viewport.
It failed before correction (session 21384, 0 passed, 1 failed). An earlier empty-header fixture had zero height and
passed: that attempt is not regression evidence. With the corrected CSS the new check passed in session 53562;
the full notes selection reported 11 passed and 1 failed. The existing initial-focus assertion failed without
identifying the active element. This run is not a successful full proof.

An instrumented repeat (session 50500) identified the first-run setup heading as the active element: the app mounted
while its checks were already running. The test-only registration import now follows the app's first mount, while
the crash handler still loads before initialization. Production does not import either module. Fresh read-only
review found no blocker. With that ordering, session 97095 passed all 12 notes checks (253.2 s total including a
full-screen hold, 29.1 s build, 1.9 s UI). This is not an app performance improvement. The infrastructure correction
is saved separately from the viewport correction. The seven lightweight proofs passed; existing source and bundle
warnings remain non-failing.

The actual hidden native app displayed Workshop update notes at 720x480 and 1280x800. Before-only instrumentation
restored the original inset on the same corrected executable; the after state used its unmodified stylesheet.
Receipts confirmed viewport top 0 before, 36 after, and titlebar bottom 36. Four final-state images were retained,
and sandbox cleanup passed with exit 0. This is a controlled regression plant, not a base executable comparison.
The capture preceded the test-startup correction. No app-executable digest was recorded at capture time; the retained
positive-build identities identify the later successful notes run, not retroactively those screenshots.

An independent read-only review by `/root/review_motion_frequent` scored the lowest before screen 39/40 and after
40/40, with no visual regression. The small before image loses one fit point for the heading under the Faber mark;
the corrected small image and both large images receive 40/40. Both captured states show Continue with focus.
Cards checked: C006, C007, C011. The automated focus regression was then resolved by the test-startup correction above.

Production changes one CSS declaration and introduces no DOM, listener, timer, process or dependency. Local runtime
memory and processor cost were not measured; the integrated memory proof remains required. Evidence is retained
outside disposable build storage at `E:/Documentos/ProjetosIA/faber-evidence/notes-window-frame-20261024/`.

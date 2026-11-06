# UX branch review verification

Review target: `55d593a`. The external report lists sixteen findings, although the request describes fifteen.
Reproductions run in Faber's hidden Windows test build; no real CLI, account files or visible desktop is used.

| Finding | Verdict and evidence |
| --- | --- |
| 1 | Real. `review 1` failed because the first answer block disappeared. Text-block boundaries are no longer answer boundaries. The engine records the first explicit final phase, or the last tool's text offset when phases are unavailable; every subsequent answer block stays prominent. Old records, unknown offsets and tool-terminated turns keep their whole text. |
| 2 | Real. A large thread with 8,855 elements reached 602 MB during repeated column toggles. The whole-stage clone is removed; offscreen messages and markdown blocks use native content visibility. The repeated-toggle screen check measures total process-tree memory, bounds remaining exit copies, and checks cleanup. A native keyboard check reproduced clipped action/link focus rings at paint-containment edges; inset outlines preserve them. |
| 3 | False. Real keyboard collapse keeps focus on the section header; searching keeps focus in search. The reported focused-child collapse cannot be triggered by those keyboard paths. `review 3` covers both. |
| 4 | False for the reported caret/scroll regression. `review 4/13` types through an active height transition, then edits in a capped scrolled textarea; caret and scroll remain stable. |
| 5 | False as a blanket jank claim. A hidden screen probe measures each fold, composer and disclosure size transition on the large thread, then welcome padding on the actual new-conversation view. It requires every transition to run, frame p95 below 25 ms and geometry reads below 8 ms. These short measured size transitions remain. The visible disclosure uses its actual position above the final answer and its answer-article containment; greeting and thread are tested separately because they are mutually exclusive app views. |
| 6 | Real. `review 6` failed on missing `role=status`. The current activity is now a polite atomic live region; only the answer text has `aria-busy`. |
| 7 | Real. `review 7` copied only “All done.” instead of the explanation and code. Copy now writes the complete original message. |
| 8 | Real. Successful conversation deletion retained its saved model/effort choice; the reproduction failed on the orphaned key. It is now removed only after successful engine deletion. The check also requires retaining it while deletion is pending and preserving another conversation's choice. |
| 9 | False. Motion preference and viewport guards run before copying; preference changes cancel animations and clean up. `review 9` verifies resize, reduced and disabled motion leave no ghosts. |
| 10 | False for the claimed leak and scroll thrashing. The original message listeners were bounded and removed on disposal; ancestor scrolling does not dispatch to descendant articles. The check verifies repeated mount/disposal and resize/scroll reads. Redundant article observers were removed to keep offscreen rendering cheap for finding 2. |
| 11 | Real. A link could acquire focus during the disclosure's closing animation and become hidden at completion. The reproduction failed before the fix; closing now returns that focus to its summary. |
| 12 | Real test gap. Synthetic animation times accepted horizontally scaled text. The new hidden screen probe samples natural frames, visible glyph width, intermediate width duration and adjacent-frame movement without pausing animations. |
| 13 | Real test gap. The original growth check disabled motion. `review 4/13` now verifies growth, capped editing and send/shrink with full motion enabled. |
| 14 | False as stated. The existing engine fixture test already exercises real Break/Text events, non-BMP UTF-16 offsets and persistence, and the existing screen check covers absent offsets. Added replay cases cover multi-block final phases, a tool after an explicit final phase, empty terminal blocks and tool-terminated turns. |
| 15 | Real. `review 15` failed with “1 steps · 1 commands”. Separate `activity.steps` and `activity.commands` use the existing plural helper in all five hand-written locales. |
| 16 | False. Cached stylesheet rules inherit current custom properties; they do not freeze theme values. `review 16` checks two successive exits after color, font and density changes against the live source. |

The initial hidden UI run reproduced findings 1, 6, 7 and 15 before their fixes. The added engine replay failed on missing boundary metadata before implementation.

With the same 8,855-element thread, sixty column toggles measured 157.4 MB before, 226.1 MB peak and 225.5 MB after;
only 39 exit elements were present at peak. Natural opening/closing travelled 344 px through 51/73 frames with
at least 120 ms of intermediate widths, unchanged visible glyph widths and 8.4 ms p95 frame intervals.
The running column no longer blurs during travel; its and the main stage's expensive shadow/rounded clipping return
at rest. Retained size transitions measured 16.6 ms p95 frame intervals and 0.1 ms p95 geometry reads after the test
fixture's separate arrival settled. All seven required proofs passed before the motion commit: 593 checks, no failures or skips.

After the disk-space interruption, proofs use `CARGO_PROFILE_DEV_DEBUG=line-tables-only` and
`CARGO_PROFILE_TEST_DEBUG=line-tables-only`. Screen checks now wait for the new document's pointer hit-test frame after
reload and for the answer body to render before reading `innerText`; all original content assertions remain.
The corrected size probe measures each size transition separately: 4.3–8.4 ms p95 frame intervals and at most 1.1 ms
p95 geometry reads in the focused run. The earlier combined fixture stacked greeting and thread and placed activity
below the answer, so it did not represent the app's views or disclosure position.
The final recovery run passed all seven required proofs: 593 checks, no failures or skips. It measured 228.8 MB peak
on the sixty-toggle fixture, 8.5–12.5 ms column frame p95, and 4.3–12.6 ms isolated size-transition frame p95.

Engine files changed for answer boundaries: `engine/src/events.rs`, `engine/src/claude.rs`, `engine/src/codex.rs`,
`engine/src/model.rs`, `engine/src/engine.rs`, `engine/src/engine/answer.rs` and `engine/tests/engine.rs`.
No dependency was added. Real account sessions and spoken screen-reader output are outside these isolated checks.

Integration with `phase-1-base` retains quota pacing, notifications, every locale key from both parents in all five
languages, and R102 effort selection. Regression replays require same-model quota recovery to preserve saved manual
effort after restart and resumed turns to keep already-visible partial answers outside the activity group, including
zero, non-BMP UTF-16 and unknown legacy boundaries. The combined UI check renders pacing, effort and grouped activity
together while keeping the final answer prominent.
The shared screen send helper clicks the textarea to establish native input focus before its single Enter; it never
retries a send. This follows a post-merge timeout with an unchanged draft before the quota scenario reached reload.
Ten instrumented focused runs did not reproduce that intermittent timeout; no product-state failure was found.
The merged tree passed all seven gates and `cargo t --all`: 655 checks, no failures or skips. Across local merge runs,
the sixty-toggle fixture's highest peak was 236.6 MB against the 250 MB limit; the final full run peaked at 219.3 MB.

The first integration CI run exposed motion checks that depended on the runner's system preference and desktop width.
The row-removal check now verifies both full and reduced motion explicitly, and the tutorial checks both ring modes.
The column fixture uses a 1440x900 viewport: the hosted desktop was below the 1100px docking breakpoint and exercised
an overlay instead. This also reproduced a real first-toggle regression after resize: the list observer retained old
viewport dimensions. Its resize listener now updates them before capturing positions, preserving the next animation.
Size-motion checks aggregate five fixed transitions and an equal idle baseline. They keep the 25 ms frame and 8 ms
geometry-read p95 budgets, require all five transitions to run, and report worst intervals rather than retrying outliers.
After these corrections all seven proofs and the separate `cargo t --all` passed, each with 655 checks and no failures
or skips. The corrected tree peaked at 227.2 MB across the complete runs; the final full run measured 216.8 MB.

The subsequent Windows CI failure was real welcome-view raster work, not a frame-count indexing error. A hidden
software-rendering replay reproduced 26.2 ms motion p95 with 0.8 ms geometry reads; pausing the ambient animations
restored fast cadence. Eightfold main-thread CPU throttling with hardware rendering did not reproduce it.
The glow now fades its composed outer layer after masking and blur, with opacity promotion only in full motion.
This preserves the picture while avoiding per-frame beam rasterization; hidden-window and reduced-motion rules still
stop it. The same software replay measured 18.1 ms motion p95 and 0.6 ms geometry reads after the change.
The layout benchmark now requests software rendering through an isolated test-only launch switch, so a fast local GPU
cannot hide this regression. Its 60-frame minimum and 25/8 ms budgets, five cycles and timeouts are unchanged.
Local validation also exposed that the runner's explicit job count overrode `CARGO_BUILD_JOBS=4`. It now caps its
half-core policy at a positive Cargo job limit; checks cover the cap, one core, and absent or invalid limits.
After cache repair and the job cap, all seven proofs and the separate `cargo t --all` passed with 655 checks, no
failures or skips. The final software welcome probe measured 15.9 ms motion p95 and 0.5 ms geometry reads.
Across glow-fix validation the highest measured thread stress peak was 215.6 MB; the final full run peaked at 201.6 MB.

Windows CI still exceeded the welcome-view active frame budget after the glow fix (26.8 ms p95), despite a healthy
17.5 ms idle baseline. A separate regression reproduced a welcome-grid observer cancelling a running FLIP on the
next container resize, even when neither child had changed its layout position: its comparison included the current
animation translation. The observer now removes tracked FLIP translation before detecting a layout jump, keeping
the existing animation alive while still retargeting genuine layout changes. The check fails before the fix and
passes after it, including an interrupted real breakpoint reversal. No frame budget, retry or timeout was changed.
The focused software-rendering replay passed with 17.1 ms welcome motion p95 and 0.6 ms geometry reads; hosted CI
must validate the remaining active-frame cost on its slower machine.
After the interrupted run was restarted, all seven proofs and the separate `cargo t --all` passed: 179 affected
checks and 656 complete checks, with no failures or skips. The full replay measured 21.1 ms welcome motion p95;
the highest post-reboot thread stress peak was 214.8 MB (223 MB after cycling all six looks), below 250 MB.

A subsequent software replay still exceeded 25 ms after the observer correction. A controlled original/cropped/
original glow comparison measured 22.2/14.7/18.1 ms motion p95: the promoted outer glow was still stage-sized although
the radial mask revealed only a small region. Its surface now matches the mask's 340 by 110 px visible bounds,
keeping the beam at the same stage coordinates and the 170 px circle centred 60 px above it. A stylesheet check
fails on the original bounds and verifies both the smaller surface and beam alignment at 720 and 1440 px widths.
The unchanged software frame benchmark then measured 13.6 ms welcome motion p95 and 0.6 ms geometry reads.
The temporary comparisons were removed; the permanent benchmark still exercises the original five transitions,
60-frame minimum and 25/8 ms budgets. No dependency, timeout increase or retry was added.
All seven proofs and `cargo t --all` passed for the bounded surface: 180 affected checks and 657 complete checks,
no failures or skips. The full replay measured 15.8 ms welcome motion p95; thread stress peaked at 216.4 MB,
and the highest six-look reading was 223 MB. The preceding observer-fix push passed Windows and macOS CI.

Historical source evidence from the original PR #323 worktree (2026-10-24). These receipts are provenance only; they do not validate this recovered candidate or its current integration run.

Recovery composition: preserve the current base's 720 px closed-after-resize reading and its first narrow-cycle limit of 5 MB over that reading; run a second identical narrow cycle with at most 5 MB over the prior closed reading. Both narrow readings remain within cold baseline +25 MB. This is deliberately stricter than the original PR's first narrow-cycle comparison against cold +25 MB.

# Visual-memory comparison at one window width

Status: complete-workload comparison passed focused native proof; integration pending. Earlier same-width candidate failed. No product implementation changes.

## Reproduction

The local integration proof for the two motion corrections passed 2,278 Rust checks and 728 UI checks, but its screen
suite finished with 126 passing scenarios and one failure. The visual version/reopen scenario retained 7.7 MB above
its warmed reading after Escape at 720 px, exceeding the unchanged 5 MB margin after 15.1 seconds. Frame weak
references were empty; the extra WebView2 process had exited by 10.7 seconds. Those facts do not prove that all
browser memory was released. No integration was queued for that attempt.

A focused unchanged-source repeat failed again: 154.0 MB versus a 146.2 MB warmed reading, a 7.8 MB difference after
15.1 seconds. It finished naturally with 0 passing and 1 failing scenario. The runner file digest stayed equal, but
normal preparatory compilation changed the app and fake CLI digests, so this is not an identical-binary reproduction.

The same existing scenario was then run from the pre-motion source, `ada2f2adb8f7de88ddc87ab146ec3e5e4c76857d`, in a
separate clean worktree. It also failed: 156.8 MB versus 148.4 MB, an 8.4 MB difference after 15.1 seconds, with empty
frame weak references. There were no changes in testkit, engine or shell source between the reference and failing
motion candidate. This demonstrates that the reported failure also occurs without the motion corrections; it does
not identify a production leak or its absence.

The logs, timing records, failed image, binary identities and natural console receipts are retained outside build
caches in `E:/Documentos/ProjetosIA/faber-evidence/landing-318-20261024-c590/`. Runs used the runner's normal resource
gate and a normal storage lease. No limit was increased and no process was interrupted.

## Comparison correction

The original first closed reading was taken at 1280 px. The second cycle changed to 720 px before reopening, so its
5 MB comparison included the cost of changing the responsive layout as well as reopening and switching versions.
A fresh read-only review confirmed the comparison mixed window widths, without attributing the full observed cost
to that difference.

An initial experiment moved resizing before the first close and asserted the original frame survived. Its native
proof failed at that assertion: the frame count was zero after resizing, before either post-close memory comparison.
That sequence was discarded. The result does not establish a permanent missing page or a production regression.
The failed experiment remains in the external `screen-corrected-first-attempt.log` and console receipt.

The proposed correction retains the original open/close/resize order. After resizing the already closed page, it
asserts there are zero frames and takes a separate reading at 720 px through the existing collection helper. This
forces garbage collection, waits at least two seconds for frame/process teardown, enforces the same 25 MB cold-to-warm
bound and 400 MB cap, and takes three process-tree samples per iteration. The second twenty-switch cycle compares against that
same-width closed reading. The 25 MB first-close margin, 5 MB second-cycle margin, weak references, process checks,
loaded-document waits, frame reuse and real keyboard/pointer interactions remain unchanged.

This changes only test sequencing and its documentation. It adds one frame-count assertion and one reuse of the
existing collection loop, no shipped allocation, process, timer or production code. The final native result below failed.
It does not prove retention is bounded across an arbitrary number of cycles, or that all responsive-layout memory
is reclaimed.

The immediate three-sample same-width experiment passed: 146.9 MB after the wide close, 151.2 MB after resizing
closed to 720 px, then 155.2 MB after the second close (4.0 MB above the narrow baseline). Total50.8s/build4.9s/screen33.3s.
A fresh read-only review noted that this new baseline had not forced garbage collection whereas the final reading
had. The final candidate therefore uses the same collection helper for both readings, including the existing 25 MB
cold-to-warm bound and 400 MB cap. The immediate-reading pass is not presented as proof of this final candidate.

## Final candidate outcome: unresolved

The same-width baseline with matching garbage collection also failed: the 720 px closed baseline was 147.4 MB
(15.2 MB above the 132.2 MB cold reading after 2.2 seconds); after reopening, twenty loaded switches, real input
and Escape, it was 154.6 MB, 7.2 MB above that narrow baseline after 15.1 seconds. Weak frame references were empty.
The process ended naturally with 0 passing and 1 failing scenario, 57.7 seconds total (5.1 build, 39.9 screen).
The earlier immediate-reading pass therefore does not establish that resizing alone explains the failure.
The candidate remains uncommitted and must not be integrated as a proven correction. No limit was relaxed.
## Complete narrow-cycle diagnostic (not a proposed integration)

A fresh independent read-only review identified a remaining workload difference: the wide cycle switches versions,
but the narrow cycle also initializes the modal host, pointer focus, keyboard input and context-menu cancellation.
The previous same-width/same-GC failure does not separate those first-use costs from repeated retention.

The next diagnostic keeps the wide workload and repeats the complete narrow workload exactly twice. Both cycles
reset their ready/focus/input/menu acknowledgements and track the current frame. The first narrow close stays within
25 MB of the ORIGINAL cold reading; the second stays within 5 MB of the first narrow close. Collection, process
identities and the 400 MB cap stay active. Additional measurements record private working sets per process, parent
renderer heap usage and parent DOM counters at opening, after switches and after input. The child renderer is opaque;
parent heap/DOM alone cannot prove its memory release. Instrumentation adds time between interactions, so a passing
result would support only this instrumented diagnostic, not automatically justify replacing the production check.

The same review found that the collection loop checked process teardown only when returning before its deadline;
after 15 seconds the final assertions omitted remaining extra processes. The diagnostic now explicitly rejects those
processes after the deadline as well. All existing callers use that same helper. No production code was changed.

The instrumented diagnostic passed naturally (1/0, 56.6 seconds total, build 4.9, screen 49.1). Cold was 137.0 MB,
closed after resize 151.6 MB, first full narrow close 158.1 MB (+21.1 cold, +6.5 narrow), second 160.1 MB
(+2.0 previous, +23.1 cold). Both extra renderer processes exited around 10.8 seconds, weak references were empty,
and closed parent DOM counters stayed 3 documents, 90 listeners and 860 nodes. Parent used heap changed from
5,124,092 to 5,140,172 bytes. This is compatible with a first-use cost but does not identify its native cause.
Source digests matched before/after. External receipts, full log and phase summary are retained with earlier evidence.

## Uninstrumented candidate

The candidate repeats those two identical narrow workloads without phase/heap probes or the extra closed-resize
measurement. The original cold-to-warm limit stays 25 MB; the repeated workload stays within 5 MB of its first
complete narrow close AND within 25 MB of the original cold value. The collection loop waits for both bounds at
once (the tighter allowance), rejects remaining frames/processes after the deadline, and retains the 400 MB cap.
The wide switching coverage, actual loaded documents and real native input are preserved. This addresses an unequal
workload comparison; it does not assert a proven browser initialization cause or absence of slow long-term growth.
The uninstrumented candidate passed naturally (1/0; 64.0 seconds total, build 4.9, screen 46.3). Cold was 135.6 MB,
first narrow close 159.3 MB (+23.7 cold), second 160.6 MB (+1.3 first narrow, approximately +25.0 cold).
Unrounded values passed both unchanged bounds. The cold allowance has little headroom; this focused pass is not
proof that all systems will pass. Extra renderer processes exited and frame references were empty at both closes.
Source digests matched before/after (16d9772a11cd62ac74e8032ee19af0d9e8ca73a9c06ecabe9780280429183420).
External evidence includes the full log, timing, console and natural-exit/source-identity receipt.

Fresh independent read-only review found no blocking code issue and confirmed the tighter combined ceiling, identical
workloads, current-frame identity and explicit process teardown. Cards C006/C009 remain satisfied; no visible product
change or shipped allocation/process/timer was added. This adds one complete narrow cycle to the screen proof,
not to normal app use. Integration suites remain pending; this does not establish a stable release.

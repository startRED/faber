# Task 3 cross-vendor review disposition

Source: `faber-notas/pesquisa/review-sd-t3.md`, reviewed against the integrated `selfdev` source.
This follow-up uses unit tests, fake CLIs and temporary directories. It does not qualify real CLI execution.
Wire/kit/bar contracts remain untouched; Task 2's real containment and exposure blockers remain binding.

| Finding | Verdict and correction / evidence |
|---|---|
| H1 | Real reset-local calibration loss; the absolute "never" claim is false because measured Chat spans can calibrate a window. Keep the same plan's history across resets; each stretch still requires one reset, complete non-overlapping spans and sufficient movement. The weekly evidence frequency remains unmeasured. |
| H2 | Real. Remove mission commitments from every Chat gate, including router, pulse and summaries. Chat journals active work; Projects yields admission/continuation. Unknown mission exposure still stays reserved. |
| H3 | Real. Every finished/failed/stopped auxiliary or Chat path retires the running count independently of cost. Recover persisted counts as unmetered spend once; snapshot unknown counters at day boundaries. Reject a missing live session before incrementing. |
| M1 | Real. Add `paid_fallback_at`, renewed only by an explicit field. Missing, future, stale or enabled paid-fallback evidence denies new mission calls and continuations (R86). Fresh quota without credit fields cannot renew it. |
| M2 | Real. Auto filters the supplied exact vendor/model/effort profile; no verified exposure bound is copied to a different profile. The trusted runner must qualify the requested profile first. |
| M3 | Real. Grant updates preserve the saved pause; only explicit Pause can resume the mission. |
| M4 | Real. Restore 200 points per window, including primary history, with no global eviction. The three-window/two-vendor serialized fixture measures 113,062 bytes; the size proof allows 128 KiB for that retained evidence. |
| M5 | Real weak proofs. Report with the changed offset and assert the resulting cost bucket; assert clock-reversed cost lands on the trusted day; deny a new grant from restored unknown exposure after cap revocation; name the pure sequence accurately and retain the actual barrier concurrency test; directly test oversized Allocate and below-gate shorter-window dollar exhaustion. |
| L1 | Real avoidable hot-path work. Chat no longer parses receipts per candidate or clones a whole ledger to start/finish a request. Pure admission no longer clones the ledger; reconciliation clones only the affected vendor. A malformed-receipt regression proves the Chat path does not parse them. Atomic persistence still serializes the ledger; safe receipt archival remains required above 4,096 invocations. |
| L2 | Real. Reports and Pause do not advance the admission clock. Reconciliation clamps booking to the last trusted admission interval and cannot poison the day counters with a future timestamp; clock reversal still denies new work. |
| L3 | False: `billing` already has `serde(skip)`. The round-trip test proves env billing is absent on disk and reloads as Unknown, requiring fresh live confirmation. |
| L4 | Real. Error reports now persist verified termination while holding unknown exposure; Continue cannot reuse that crashed invocation. |

## Previously unconfirmed points

| ID | Verdict and evidence |
|---|---|
| U1: provider reset jitter | Unconfirmed provider behavior. Exact reset identities remain conservative; a changed weekly identity requires fresh boundary evidence. No invented tolerance or capacity. |
| U2: credit fields on every reading | Unconfirmed provider frequency. Parsers permit omissions; the regression proves missing fields never renew the evidence timestamp. Explicit fresh false is necessary for mission admission. |
| U3: weekly calibration frequency | Unconfirmed in real accounts. Keep ENOUGH and movement requirements; Chat evidence can accumulate across resets, and absent evidence parks automation. |
| U4: multiple-turn reports | False within the documented contract: every model request/resume has a new invocation ID. Separate Codex turns in one session settle as separate once-only receipts; replaying either does not replace or double-charge the other. T5 must preserve this scope. |
| U5: non-terminal Stop | T5 interpretation remains unconfirmed because its runner is not implemented here. A separately reproduced status ambiguity is fixed: complete live cost returns Ready while keeping exposure held; unknown live/terminal cost returns Stop. T5 must treat Stop as a stop request, never permission. |
| U6: separate engines | Real with a custom data directory: the single-instance plugin is bypassed. A native lifetime file lock rejects a second ledger writer, preventing overbooking and stale overwrites; shutdown releases ownership. The regression uses separate engine instances and real OS locks without spawning a process. |

## Proof

Regression tests were run red against the old guards before correction. All 20 focused guard-removal mutations
failed their named assertions, including R86 freshness/enabled-credit denial, exact profile qualification, pause,
retained history, allocation/short-window/offset/clock/pending-exposure checks, crash/recovery, future timestamps,
once-only receipts, live status, directory ownership and the merged warm-session lifecycle. Compilation errors do not count as killed mutations.
The [recorded results](2026-10-12-task-3-guard-mutations.json) list each target and exit status; source files were restored.
The Task 2 idle-interrupt test initially mistook Task 3's expected post-settlement billing-Unknown signal for process death.
It now accepts only that signal while still rejecting lifecycle events and checking the next wire turn. Removing
the driver's completion guard makes it fail; no timeout or retry was changed.
Provider timing, live containment, invocation ceilings and stop
latency are still Task 2/5/13 qualification work; no real model session or paid credit is authorized here.

Local proofs: lint, typecheck, build, fmt, both clippy commands, `cargo t` and `cargo t --all` all exit 0.
Both test runs report 782 passed, zero failed/not-run/skipped. Hidden desktop probes see zero visible windows;
idle memory is 108-112 MB, six-theme memory 186 MB, and the thousand-message thread 141-144 MB.
The existing four-week pacing smoke simulation passes; no pacing constants or learning history limit were reduced.

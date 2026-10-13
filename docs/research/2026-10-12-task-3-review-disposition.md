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
The [recorded results](2026-10-12-task-3-guard-mutations.json) list each target and nonzero-exit result; source files were restored.
The Task 2 idle-interrupt test initially mistook Task 3's expected post-settlement billing-Unknown signal for process death.
It now accepts only that signal while still rejecting lifecycle events and checking the next wire turn. Removing
the driver's completion guard makes it fail; no timeout or retry was changed.
Provider timing, live containment, invocation ceilings and stop
latency are still Task 2/5/13 qualification work; no real model session or paid credit is authorized here.

Local proofs: lint, typecheck, build, fmt, both clippy commands, `cargo t` and `cargo t --all` all exit 0.
Both test runs report 782 passed, zero failed/not-run/skipped. Hidden desktop probes see zero visible windows;
idle memory is 108-112 MB, six-theme memory 186 MB, and the thousand-message thread 141-144 MB.
The existing four-week pacing smoke simulation passes; no pacing constants or learning history limit were reduced.

## Independent verification follow-up (N1-N6)

| Finding | Verdict and correction / evidence |
|---|---|
| N1 | Real. Removing a conversation records its unfinished turn before closing the session. The regression asserts one retired active count, one unknown-cost receipt and one close, including repeated removal. |
| N2 | Real. Separate permanent Projects closure (missing ownership, incompatible/corrupt authority or unrecoverable metadata) from retryable ledger writes. Chat continues in memory; Projects still denies, and Chat cannot overwrite another owner's or an unreadable budget. Tests cover a second engine, newer schema, bad receipts/active counts, repeated turns and a failed journal write followed by recovery. |
| N3 | Real for pruned historical counters. Compare missing baseline counters only within the current day's possible local offsets, matching the existing spend check. Current-day counter loss still denies. The old-baseline claim is not a bug: an absent unknown-cost snapshot provides no evidence that current-day unknown spend preceded the baseline, so denial remains necessary. |
| N4 | Real abandoned-window retention. Retire an unreported, expired window only after its last point is over 30 days old; retain all 200 points for every active window and recent reset. The regression checks both retention and eventual expiry; the existing primary-history proof remains unchanged. |
| N5 | False as a T3 defect: the supplied exposure qualifies exactly one vendor/model/effort. Removing identity checks admits an unqualified profile. Selecting across multiple independently qualified profiles remains T5's caller responsibility; no bound is transplanted and no wire/kit/bar contract changes. |
| N6 | Real for delayed receipts with fresh quota evidence. Fresh readings extend the trusted booking interval, preserving the actual day and span end. Without newer evidence the conservative clamp remains; an explicit future-day assertion protects against clock glitches. |

N1-N4 and N6 regressions failed against the previous implementation before corrections. Guard-removal runs
also cover N5's existing qualification boundary and N6's future-clock protection; compilation failures are not
accepted as evidence. Results are in [the follow-up mutation record](2026-10-12-task-3-follow-up-mutations.json).
No real CLI session, paid credit, visible window, credential or installed app was used.

The first broad run hit a compiler crash while the runner overrode `CARGO_BUILD_JOBS=4` with eight jobs.
A native probe showed Windows still reports 16 cores under process affinity, so affinity did not enforce the
requested limit. The runner now honors positive `CARGO_BUILD_JOBS` values, keeping its half-core default when
unset. Its regression failed first at 8 versus 4 and checks both parsing and the actual `jobs()` call; removing
the wiring fails it under the authorized two-job environment. No retry or timeout policy changed.
The full engine suite, ten mutation checks, lint, typecheck, build, fmt and both clippy commands pass.
Before the reboot, even a direct two-job build exhausted Windows commit memory while
compiling the desktop crate (`memory allocation of 1982464 bytes failed`). The first runner attempt passed its
77 UI/screen checks, but its Rust build failed. The reboot restored memory headroom; the four-job environment is
used again. Real model/paid-credit behavior remains unproven.

The resumed full run passed 788 checks but failed one process audit after the reconnect flow itself passed.
Its surviving-process report named a console host and Windows Update. A pure red regression reproduces the
audit's ability to claim an unrelated successor of an unfinished fake CLI's PID. Start and exit records now
include creation time; both the running-record filter and leak check require that identity. Missing timestamps
are errors, and real recorded survivors still fail. Native creation time also exposed the WMI wrapper's loss of
CIM fractional precision; the snapshot now preserves the original six microsecond digits. Its native/snapshot
identity test failed before correction. Guard-removal checks cover all three audit corrections. Cleanup still
ends only owned jobs; no unrelated process is killed and no timeout or retry policy changes.

Post-reboot proof: all seven AGENTS.md commands plus `cargo t --all` exit 0 with four jobs and line-tables-only
debug info. Both runner commands report 792 passed, zero failed/not-run/skipped. All 13 follow-up mutations fail
their named assertions. Hidden probes report zero visible windows; idle memory is 108-110 MB, six-theme memory
183-185 MB, and the thousand-message thread is 140-142 MB.

Integration with `origin/selfdev` brings the accepted T5K and T11 work unchanged. The two conflicts preserve
the native completion guard (`finished` is the same result predicate), quota-report tracking and both lifecycle
tests; the runner uses the base's half-core ceiling plus the configured positive limit and retains both test sets.
Both focused integration tests pass and both guard-removal checks fail. There are now 15 follow-up mutation
results, including these merge checks. Wire/kit/bar changes come only from the accepted base.

Integrated-tree proof: all seven AGENTS.md commands plus `cargo t --all` exit 0. Both runner commands report
900 passed, zero failed/not-run/skipped. Hidden probes still show zero visible windows; idle memory is 107-112 MB,
six-theme memory is 186 MB and the thousand-message thread is 140-142 MB. Four jobs and line-tables-only debug
info remain in use.

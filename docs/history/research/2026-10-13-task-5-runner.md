# Task 5: guarded mission runner and fixture acceptance

Date: 2026-10-13. Branch: selfdev-t5. Status: fixture implementation; automatic admission stays closed.

## Boundaries

The runner uses T1's journal, T2's non-configurable admission gate, T3's shared mission ledger,
T4's repository lock/leases/CAS broker, and T5K's wire v2 and frozen kit receipts. It does not grant
native tools, relax role restrictions, release live or unsettled leases, install anything, or use the
legacy owner-checkout session as a fallback. All automated roles remain denied by T2.

T5B owns the complete software compiler and bar. `lead::accept` and `Runner::install_plan` require
its compile-receipt validator; `Runner::land` requires its candidate bar validator. Missing validation
is an error. No files belonging to the software-bar implementation were changed. The native fixture
supplies a deliberately small first-party check and a fixture review, not the complete software bar.
T9 consumes the runner state and completion verification result; T11 consumes accepted trees and
keeps ownership of its current/next/last-good slots. No local update or installed-release action runs here.

## Implemented contracts

- Stable persona on both shared driver start/resume paths, including unnamed conversations;
  side questions keep their own instructions. Workers receive the role contract, not the persona.
- Exact UTF-8 requirement/check/quote rendering, strict envelopes and bounded notes, two-bounce
  closure, and test proposals stored as artifacts rather than source actions.
- Explicit model/effort/version pins and per-turn reported-identity checks. Proof failures escalate
  effort then model with a fresh typed handoff and shrinking attempt allowance inside the lifetime cap.
- Persistent proof/hunk reflexes and one continuation before a stalled completion blocks.
- Event-driven readiness with critical-path/value/age ordering, cost/daily/RAM/reviewer brakes,
  a bounded queue and one source writer. Independent branches remain eligible.
- Named executable/argv proof definitions and engine observations bound to exact tree, input,
  changed paths, toolchain, dependencies and environment. Clean candidates use `cargo t --all`.
  One heavy proof owns the build lane. Missing, failed or unknown observations cannot be reused as pass.
- Serial exact-tree proof before fresh other-vendor review. Changed trees invalidate both gates.
  The broker checks the proven tree again before commit and retains its existing CAS/grant checks.
  Landing intent ids survive recovery; known authoritative receipts are read instead of repeating effects.
- Atomic runner checkpoints, pause without completion, quota/promotion cancellation, bounded graceful
  session close, and no PID adoption. Write faults halt admission; uncertain effects park recovery.
- A Scar-only store accepting matching red/green program receipts. Scars expose applicable check ids,
  never prompt rules or failure text. Route v0 is a typed report only, explicitly labelled fixture or live;
  the scheduler never reads it as a plan.

## R106 disk ownership

Native free-space sampling covers private, repository and recorded git-common write volumes at build admission.
Durable build reservations count unmaterialized bytes against native free space and the mission disk limit.
Unknown readings, insufficient reserve or a
mission disk-limit overflow pause admission before any build resource is created. Checked arithmetic
cannot wrap into extra capacity. Owned worktrees count alongside build, temporary and log resources.

Resource consumers are released durably before deletion. Live trees, mismatched process start times,
unsettled costs, unresolved intents and links retain their data. A completed local landing releases its
known lease, removes the exact broker-owned worktree, and deletes unused build/temp/log resources and
obsolete private patch backups. Public records and authoritative safety ledgers remain. No shared
Cargo cache, owner checkout, installed release or running executable is a cleanup target.

Retirement checkpoints survive candidate removal and replay known deletion receipts without repeating git effects.
The same seam retires settled failed, stalled, abandoned and rolled-back attempts after their final consumer releases
them, retaining the discarded candidate as an artifact. Different process owners and unresolved effects remain parked.
See the [review disposition](2026-10-13-task-5-review.md) for reproductions, false subclaims and mutation evidence.

## Evidence and remaining qualification

The regression tests were written before implementation and observed red. Replays cover worker
identity, verbatim bytes, malformed/stale packets, loops, stalls, shrinking retries, scheduling brakes,
proof receipts, fresh review, stale heads/grants, recovery, disk admission and retirement. The hidden
native fixture checks a red-to-green source change, composed tree, local landing and automatic cleanup
while preserving a dirty owner file, owner index and owner ref. It spends no model quota.

The base merge also exposed a pointer regression: two stable frames did not mean a finite arrival
animation had finished. A deterministic hidden regression failed before restoring the existing motion
wait while retaining the new rendered-layout/hit-test check. No timeout increase or retry was the fix.
The native compose fixture exposed Windows extended paths in `GIT_INDEX_FILE`; native argv/environment
spelling is adjusted after the broker validates the canonical path.

Unproven: G1/G2 official-CLI restrictions/exposure and non-Windows owned-tree containment; arbitrary
candidate-script write/network containment; live G8, paired persona cache-read share and performance;
complete T5B integration and real cross-vendor review. Fixtures do not qualify these controls. The
runner's dispatch-preparation seam cannot start a role while T2 refuses it. Production role/executor
assembly must use the qualified driver constructors and the complete bar, never fixture observations.

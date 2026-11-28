# Task 5 review disposition

Date: 2026-10-13. Reviewed input: `faber-notas/pesquisa/review-sd-t5.md`, commit fe18b8e.
The second review is pending; PR #7 remains a draft. No official CLI or model session was run.

## Findings

| Finding | Reproduction and disposition | Change / regression |
|---|---|---|
| H1: candidate popped before retirement | Real. A fixture write fault at lease release leaves Done and an empty queue with a held worktree. The original checkpoint has no retirement record. | Persist the exact lease, process identity and stable cleanup intent before popping. Explicit recovery releases/archives only with known termination and settlement, reuses the authoritative deletion receipt, and retries local resource retirement. |
| M1: stale head/grant on recovered commit | Real runner validation gap; the claimed bypass of T4 is false: T4 rechecks grants and uses CAS. The original stale revision test returns reason.grant.stale before any git call. A previously forwarded receipt also lets the runner ignore a moved own head. | Validate current revision/head before the bar and reuse; recheck T4 authority even when git receipts already exist. A forwarded receipt expects its new head; a commit-only receipt expects the old base. |
| M2: volume sampling and overcommit | Real. Repeated 60-byte admissions into a 100-byte limit both succeed without reservation. A full repository volume is ignored by the original private-only sampler. | Sample private, repository journal and recorded git common directories. Persist build reservations; count max(actual,reserved) for the mission limit and subtract unmaterialized bytes from native free space. Unknown/full readings pause before admitting a build. |
| M3: cleanup on success only | Real for settled attempts after their final consumer releases evidence. Fault/stall/abandon/rollback fixture paths now retire worktrees and resources without turning blocked work into Done. The demand to clean close_abandoned with unknown spend/pending effects is false: T4 explicitly retains uncertain evidence and must not be weakened. | Add durable retire_task and recover_retirements seams; quarantine can retire only after T4 archive records exact generation/process/dead-tree/settlement proof. |
| M4: resource/process mismatch | Real for process-free output. The original cleanup cannot delete a None-owned temporary resource using a worker termination identity. Rejecting a different PID/start time is correct, not a bug. | Use None for process-free outputs; require an exact matching proof for process-owned outputs. Failed retirement remains checkpointed after landing. |
| L1: duplicate native path spelling | Real structural duplication. A source regression detects a second UNC-prefix implementation. | Worktree creation calls the same native_path helper as index and retirement. |
| L2: duplicate temporary-root helper | Real structural duplication. A source regression fails with any of the three copied helpers. | Reuse engine/tests/support::tempdir, preserving canonical temporary roots on macOS. |
| L3: vacuous assertions | Partly real: the combined disk test covered no build output. The Scar and route assertions are not vacuous: adding a rule or writing a plan would violate them. | Count worktree plus a registered build output exactly; inject an unknown Scar rule and reject reopening; seed a real plan and compare unchanged bytes before/after rejected and accepted route reports. |

## Partial coverage

Recovery and R106 now have durable fixture seams and regression evidence for post-landing crashes,
settled non-success retirement, multiple write volumes and outstanding build reservations.
Uncertain intents are still parked, never repeated. Reused PIDs, unsettled spend and live/different owners
retain their files. The authoritative effect/runtime records and public evidence are never deleted.
Automatic role execution and G1/G2 cannot be qualified by fixtures: the owner prohibits real model sessions.
Both vendors remain denied by T2, and candidate-script containment remains unproven. T5B files and its
mandatory compiler/bar callbacks remain untouched. No live G8 acceptance is claimed.

The non-success regression also checks removal of obsolete patch backups after T4 archival proof.
Full abandoned-candidate evidence remains private; the public artifact uses an explicit field allow-list,
so review session identities and future private extension fields are not exported. Both checks were
observed red before correction. A forwarded receipt without its commit parks recovery before new effects.
The public copy uses T1's Artifact schema and generation-based ids, including 64-character task ids.
Resume recognizes T4's known archival termination proof without releasing quarantine or restoring source authority.

## Evidence

Focused tests were observed red before their fixes. The two duplication checks are structural; behavioral
checks use fixture executors, injected disk failures and recorded free-space samples. Mutation replays
remove individual protections and require the corresponding regression to fail without compiler errors.
The full required local proof set and Windows/macOS CI results are reported on PR #7 after verification.

Local verification: native lint/typecheck/build executables (no shell children), format check, both clippy checks,
`cargo t` (881 passed), and final `cargo t --all` (991 passed: 798 Rust, 108 UI, 85 screen). No failures or skips.
Thirteen individual mutation replays failed their target regressions with the fix removed and without compiler errors.
Additional red/green regressions cover private session metadata, T1 artifact decoding/id bounds, obsolete patch backups,
and known archived process identities. CI is checked on the pushed commit; this document does not qualify live gates.

PR synchronization required merging `origin/selfdev` at 974637b, which already contains accepted T5B and T9.
The merge appends T5 helpers to the shared lead/proof modules; bar.rs, native/screen proof executors and T5B tests
remain identical to the target branch. Both finite-arrival and post-hover pointer checks remain before one click.
One duplicate answer_started fixture initializer introduced by the merge was removed after the compiler rejected it.
All seven proof equivalents pass on the integration, including cargo t and cargo t --all: 1,110 passed in each run,
zero failed, not run or skipped. No real model session or paid credits were used.

Windows CI on the integration exposed a Projects evidence assertion racing the queued native toggle event.
Controlled event delivery reproduced the exact missing-scope failure locally before the correction.
The scenario now waits for the rendered scope paragraph with the existing deadline after one pointer click,
then checks the original evidence assertion. The controlled regression passes with this readiness check
and fails without it; no retry or timeout increase is involved.

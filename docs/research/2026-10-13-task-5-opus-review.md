# Task 5: independent review and Projects commands

Date: 2026-10-13. PR #7 remains a draft. All observations use fixtures, injected disk faults or the hidden native
test sandbox. No model session, paid credits, installed release or existing Faber process is touched.

## Findings

| Finding | Result and regression | Change |
| --- | --- | --- |
| H1 | Real: `opus_h1_split_checkpoint_recovers_authoritative_state` and `opus_h1_external_mirror_write_cannot_override_runner` failed with `error.store.corrupt`. | `run.json` is authoritative. Load repairs the readable mirror and preserves newer schema errors. Legacy T1/T9 records are adopted once. All screen mutations call the runner. |
| H2 | Real, latent behind T2: `opus_h2_settled_dispatch_closes_before_resume_and_cleanup` failed at Resume with `reason.project.reconcile`. | Trusted native start/failure closes read-only dispatch receipts. Known priced and terminated ledger reservations reconcile legacy dispatches. Writer admission consumes T4's existing generation intent instead of creating a second intent; T4 still owns attachment and termination. Unknown spend or process state stays parked. |
| H3 | Real: `opus_h3_landing_checkpoint_recovers_without_requeued_retirement` retained a queued candidate after cleanup. | Queue removal, completion state and durable retirement identity share one private checkpoint. Cleanup retains its stable receipt and is idempotent across a fault on the next checkpoint. |
| M1 | Real: `opus_m1_stop_resume_reopens_only_reconciled_budget` found the grant still paused. | Authorized Resume re-reads current authority, reconciles effects, leases, heavy proofs and unsettled spend, then reopens the grant. A failed checkpoint closes admission and attempts to pause again. Public Resume also checks T2. |
| M2 | Real: `opus_m2_heavy_lane_spans_candidates` started two heavy sets. | Only the runner can start a proof. Its durable heavy slot binds task, proof, tree, inputs and definitions across candidates/reload; a matching engine receipt releases it. |

`opus_m2_known_proof_receipt_recovers_after_checkpoint_fault` additionally proves that a completed heavy proof's
durable receipt releases the slot after a failed runner checkpoint; missing evidence keeps the slot parked.

Each finding was observed failing before its fix and green afterward. Eleven mutation replays independently remove
checkpoint repair, dispatch settlement, budget Resume, the shared heavy lane, atomic landing, Stop budget/interrupt/close,
the bounded role queue, the screen's runner mode call and heavy receipt recovery. Each fails its target regression without a compiler error.
An additional red/green check keeps private CLI session identities out of public dispatch receipts.

## Screen integration

Start, Resume, Stop, mode and answer use the runner API. Read paths recover the readable mission mirror from the private
checkpoint; an unknown or empty public directory creates no private authority. Mode and answer preserve monotonic view
revisions. Start checkpoints Planning after current grant and T2 admission; it does not claim that a model has answered.
T2 refusal reaches the screen as its reason key, never `runnerUnavailable`. Seguro's remote denials are unchanged.

Stop closes admission and quota/promotion wakeups, pauses the shared budget, checkpoints, interrupts engine-owned mission
sessions and closes them after the grace period, including on a checkpoint fault. It never releases a lease or a cost
reservation. The engine and screen share the same asynchronous mission command lock. Dedicated role constructors mark
their sessions; legacy Chat sessions cannot be adopted as role sessions. A bounded, ordered event queue feeds the
runner and ledger. Its temporary working directory is registered with R106 resource ownership.

`screen_stop_interrupts_only_owned_mission_sessions_even_on_checkpoint_failure`,
`screen_dispatch_uses_t2_before_any_session_or_spend`, `role_event_order_and_bounds_preserve_unknown_reservations`,
and the native `projects T5: mode, answer and Stop use the authoritative runner` cover these boundaries.

## Still partial

T2 remains closed for both vendors. The owner prohibits real model sessions, so G1/G2 and live mission qualification
cannot be claimed. The qualified writer launcher still needs T4's owned process identity; the dedicated adapter refuses
that missing evidence. Planning/lead driver assembly and execution of accepted envelopes need the qualified driver and
action bridge. Candidate-script write/network containment, live complete-bar/G8 acceptance and cache/performance
measurements remain unproven. T5B compiler/bar and native/screen proof executors are untouched. No fixture unlocks them.

Before the fix commit, all seven proof commands passed (native npm script executables avoid shell children).
Both `cargo t` and `cargo t --all` passed 1,123 tests with zero failures or skips. Eleven mutations failed their
target regressions. The merged base and pushed head are validated separately in PR #7.

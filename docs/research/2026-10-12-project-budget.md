# Mission spending adapter (T3)

T3 adds admission and attribution to the existing Auto pacing and `budget.json`. It does not launch a worker,
classify a brief with a model, install tools, or change the wire/kit/bar contracts.

## Runner handoff

- `Engine::mission_budget` applies an explicit daily allocation, mission/vendor/worker caps, protected review,
  repair and canary room, admission, continuation, pause or usage reconciliation. It persists under the existing
  engine lock before an effect is permitted. Failed writes and corrupt safety records deny admission.
- `Engine::mission_route` filters the existing Auto candidates through mission admission and uses the same paced
  table, including its vendor pin for independent review. Its caller supplies the classified brief and an exact qualified
  vendor/model/effort invocation; its exposure is never transplanted to another profile. Routing here spends no quota.
- Invocation IDs are durable and cannot be dispatched twice. A worker ID stays fixed across attempts, days and vendor
  handovers. Reports use the existing token prices, cumulative session settlement, own-spend counters and learning spans.
- T5 must obtain `exposure_micros` from a qualified T2 driver bound, including nested calls and stop latency, and must
  check continuation before every tool effect and on usage changes. A model estimate or CLI budget flag is not that proof.
  Only the trusted coordinator calls these hooks; worker packets cannot allocate grants or certify exposure.
- Every new model request, including resume, needs a fresh invocation ID and reservation. Continuation only checks the
  already-reserved invocation; it never renews its maximum. Session keys must remain bound to the same CLI session;
  unexpected session changes retain unknown exposure. T5 rechecks the chosen model/effort's qualification before admission.
- Final counters alone do not release a live process's reservation. Missing, incomplete or differently pinned rates
  keep exposure held. Reports still reconcile while paused, exhausted or recovering from a clock reversal.
  Breaches or corrections to an already-final receipt disable that vendor/model/effort configuration.

## Shared allowance

The hard Projects gate derives each daily boundary from the existing weekly reset and 1/7 line. It needs a fresh
reading, evidence at or just before that boundary, a rounding guard, confirmed subscription billing with paid fallback
disabled with its own fresh explicit evidence, and enough complete non-overlapping calibration stretches for the selected model. Shorter/family windows
also constrain admission. Missing evidence parks work; T3 runs no calibration probe to obtain it.
Each measured interval stays inside one reset. The same plan's calibration survives resets; a plan change invalidates it.
Existing Auto keeps all 200 readings per window; another window never evicts its primary history.

Quota samples snapshot the existing own-spend counters. Their deltas measure reset-relative own cost even when the
local offset changes. Outside account movement reduces room but is never booked as a worker's own dollars.
Allocations pin `usage::RATE_DATE`; a new day requires an explicit bounded allocation and preserves lifetime receipts.
Mission/vendor subgrants count once inside that shared allowance, including protected acceptance work.

Chat, routing pulses and handover summaries keep their existing unbounded requests and always take priority over missions.
Mission grants never block Chat. Before sending, the engine journals active requests against the same ledger; Projects
cannot admit or continue on that vendor while they run. Every terminal path retires the active count, even with unknown
cost. A crash retires persisted active counts into existing unmetered spend. Fresh day baselines include those counters,
so yesterday's uncertainty does not become a permanent running request. Unknown mission exposure remains reserved.
Only explicit Pause changes an existing mission's pause; top-ups and new-day grants cannot override the person's Stop.
An OS file lock gives each spending directory one engine writer, including builds that bypass the single-instance plugin.

## Evidence and limits

The spend-path tests were written before implementation: the initial admission stub failed 13 of 14 checks.
Additional red regressions exposed final-cost correction, daily-cap reduction, corrupted safety records, clock reversal,
local-offset changes and next-day renewal. The suite also covers every mission spend kind, cache pricing, resume bases,
vendor handover, protected reserves and once-only settlement. Engine tests exercise concurrent handles, durable dispatch,
Auto reuse, failed writes and unresolved Chat usage without spawning a process.

This is source and fake-CLI evidence. Real CLI containment, invocation bounds, calibration and canary qualification
remain T2/T5/T13 gates. No real model session or paid credit was used. Invocation history is bounded at 4096 receipts;
admission closes at that limit until safe archival is implemented. No release or installed Faber was touched.

The cross-vendor review's disposition and guard-removal proofs are in
[the follow-up report](2026-10-12-task-3-review-disposition.md).

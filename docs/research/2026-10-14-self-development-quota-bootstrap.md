# R130: measured readings before quota conversion exists

Owner decision R130 permits a project mission with missing model/window conversion only after a fresh, zero-model-turn
account reading after the current reset-relative daily boundary, with at least five percentage points below that day's
cumulative weekly line for every vendor used. The mission cap remains USD 2.00. This is permission to collect the first
measurement, not a guessed dollar-to-quota conversion.

## Admission and recording

The shared project budget checks this exception during allocation, grant creation, routing and turn admission. Every
vendor in a mission is checked again before a new turn. Existing measured windows still restrict headroom; missing
windows do not erase a measured restriction. Billing must be subscription and fresh paid-fallback evidence must say
false. Active foreground work, unknown exposure, purpose caps, protected review/repair/canary funds and lifetime dollar
caps retain their existing checks. Bootstrap grants cannot exceed USD 2.00 per mission. A daily allocation may include
already settled own spend plus up to USD 2.00 of remaining bootstrap room, so a preceding small canary does not silently
shrink or raise the next mission's USD 2.00 cap. Concurrent commitments still share that remaining room.

Only the engine's no-model account checks stamp zero-turn provenance. Ordinary conversation quota events and
`note_account_events` cannot stamp it; invocation extras cannot supply it. The first actual weekly sample is saved as
the day baseline, including the existing own-spend and unknown-spend counters. A subsequent refresh cannot replace that
first baseline with an invented empty day. A plan change requires a reading from the current plan.

Allocation for a selected model uses `Change::AllocateModel`; a conversion for another model cannot replace that
model's missing measurement. The zero-turn admission probe selects the actual mission models. Existing vendor-wide
allocation remains conservative and unchanged for calibrated callers. A model-specific turn also enforces the original
USD 2.00 mission cap, even when the grant was created while another model was calibrated. First endpoints are captured
at grant creation when available, or at the first bootstrap-admitted turn; caller metadata supplies no measurement.
On a new day, validation uses a staged actual baseline before accounting yesterday's spend, and commits it only after
all checks succeed. Failed allocation leaves the ledger byte-for-byte unchanged.

The host calls `Engine::calibrate_mission` after retirement. It refreshes both accounts and records actual before/after
readings plus exactly-once settled mission costs. An unfinished or bound-charged invocation cannot become a calibration
stretch. Reset changes, stale endpoints, quota decreases and missing counters are rejected. Zero quota movement is
recorded as zero, and produces no conversion. Mixed-model vendor use retains the raw evidence but produces no invented
per-model split. A positive same-model measured stretch enters normal R88 admission immediately; the largest observed
points-per-dollar conversion is used conservatively alongside the existing normal stretches. Any outside use within
the measurement interval can increase this conservative rate; it never increases mission dollar headroom.

## Installed no-model query

The documented structured `/usage` report is available in Claude Code 2.1.296. The account query retains the shared
strict empty MCP, disabled hooks/plugins and empty tools controls, enables only built-in slash-command dispatch, and
sends `/usage`. It requires a terminal `local_command: "usage"`, zero total cost and empty model usage; an assistant
without a usage report, a streamed model response or a control request stops the query. The query closes its owned
process tree and writes no session. Reset timestamps are parsed as checked UTC calendar dates, at whole-second
precision. Unknown scoped display labels are not guessed as model identifiers.
The documented "last-known usage" fallback is rejected: completing a new local command cannot make cached server data
fresh. This has a regression in `r130-red-cached-usage.log`; the original implementation incorrectly accepted it.

Source: [official SDK reference](https://code.claude.com/docs/en/agent-sdk/typescript#sdkusagereport). The installed query
was checked live, not inferred from documentation. Codex keeps its existing `account/rateLimits/read` no-turn query.

## Evidence before the requested boundary

Private root: `C:/Users/Erick/AppData/Local/Faber/t13-exposure-20261014/`.

- `r130-red.log`: the new valid-reading regression failed on the previous admission path (`reason.budget.day`).
- `r130-green-budget.log`: the budget regressions and existing budget tests passed during development; final mandatory
  proofs are recorded separately before the commit.
- `r130-red-prior-spend.log`: the preceding measured canary cost initially shrank the next mission's room incorrectly;
  the regression now preserves the USD 2.00 mission cap while retaining that known cost in the daily allocation.
- `r130-red-revoked-calibration.log`: revoking a finished mission's spending grant initially made its historical
  calibration look corrupt. Historical measurements now survive revocation; current authority never rewrites them.
- `r130-red-review.log`: cross-file review reproduced yesterday's unknown-spend baseline blocking a valid new day,
  and another calibrated model suppressing the new model's first calibration. Both are covered by regressions.
- `r130-red-model-cap.log`, `r130-red-late-model-reading.log`, and `r130-red-selected-model.log`: regressions for the
  model-specific cap, the first fresh reading arriving after grant creation, and allocation without another model's
  day baseline. `r130-green-selected-model.log` records the full budget suite after the fixes.
- `r130-red-slow-quota.log`: the new quota query initially delayed publishing already completed subscription detection.
  The regression proved this could block Auto Chat while a quota query waits. Account facts are now published first;
  quota provenance is stamped only when the subsequent query completes. `r130-red-slow-quota-other-vendor.log` also
  reproduced the delay of the other vendor's account check; both account checks now precede optional quota work.
  The original screen timeout is retained in
  `validation-r130-complete/proof-07.log`, not silently counted as a passing proof.
- `r130-claude-quota-before/live-denial.json`: live local `/usage`, zero model turns, USD 0, zero-turn terminal receipt,
  marker bytes and hash unchanged; weekly usage 31%, reset 2026-10-20 08:00:00 UTC; paid fallback disabled.
- `r130-engine-before/zero-turn-admission.json`: the real engine's account checks and shared allocation path, both vendors
  on subscription, both paid fallbacks false, no model turns. Claude weekly usage 31%; Codex 35%, reset
  2026-10-20 04:58:09 UTC. At this reading both daily lines were 28.5714%, so both correctly refused R130 admission.

These are observations before the requested 2026-10-15 04:58:09 UTC boundary. They are not substituted for the required
fresh readings after it, not a real mission and not a conformance receipt. Receipt admission, writer retirement, real
canary, build/install, rollback and positive Resume remain separate evidence requirements.

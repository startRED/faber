# Task 13: R120 role output controls and sub-caps

Observed on 2026-10-14. This replaces the R117 role-sub-cap stop, not the
remaining qualification, quota/calibration, canary or mission gates. The owner
authorized rebalancing within $2.00 and continuing proven steps without waiting
for CI between commits (R119/R120). The separate review of d66c472 and b942eef
found all five checks sound; none is rejected.

## Supported controls and the live recovery gap

Claude Code 2.1.296, executable SHA-256
`76de74f7694a5bdc03378a5f837c9e132827bf004c98c276bc4e9094ff06769a`,
supports `CLAUDE_CODE_MAX_OUTPUT_TOKENS`. The shared project builder fixes it at
8,000 for Lead, Research, TestProposal and Reviewer, and 32,000 for Writer,
including resume. Prompt caching stays disabled. A fake-builder regression
first failed because Writer still received 128,000.
The production builder also disables compaction, transport retries and
nonstreaming fallback through the documented controls already exercised in the
512-token experiment. The all-role/start/resume regression first failed on the
missing `DISABLE_COMPACT` value; the diagnostic no longer injects those controls
independently of production.

A short Writer turn returned 67 output tokens for $0.0001475. Its reported
`modelUsage.maxOutputTokens: 128000` is the model capability, not the configured
request maximum. A forced 512-token request exposed output-recovery calls:
four requests produced 2,048 total output tokens despite `--max-turns 1`.
Disabling network retries did not stop that recovery. These are retained failed
observations, not passing invocation bounds.

The documented `--max-budget-usd` is checked between API requests, including
recovery. With a $0.0003 estimate threshold, the same forced request stopped
after one 512-token response at $0.0003710. Top-level terminal counters were
zero; the verifier now uses the nonzero whole-model counters and requires one
streaming request. Its regression rejects missing counters and multiple
requests. This threshold is not a hard spending cap: the computation reserves
one entire overshooting request.

Every project invocation now uses `--max-budget-usd 0.005` and `--max-turns 1`.
The estimate threshold's settled spend is conservatively bounded at
$0.005 × 5.5 = $0.0275, covering the published long-context/base price ratio and
10% US geography premium. The full final request is priced separately,
uncached, at the documented context envelope with that premium. Additional
last-settled spend is added; interrupted attribution is labelled **bound**.

Sources: [environment controls](https://code.claude.com/docs/en/env-vars),
[CLI budget semantics](https://code.claude.com/docs/en/cli-reference),
[usage fields](https://code.claude.com/docs/en/agent-sdk/typescript),
[dated model prices](https://platform.claude.com/docs/en/about-claude/pricing).
Full role qualification is still required before this bound can authorize a
worker. The output-control diagnostic is not an admission receipt.

Codex 0.162.1, executable SHA-256
`dd13bdb162a184fa04039488b234a8258ef9b76f84e926c505d5742b19249a6a`,
freshly generated its app-server schema without a model turn. Neither that
protocol nor its version-matched config schema exposes a per-request model
output maximum. Tool-output truncation and completed-usage rollout budgets
are not claimed as that control. Luna therefore retains its documented
128,000-token output maximum and 1,050,000-token context envelope, default tier.
[Installed-version config schema](https://raw.githubusercontent.com/openai/codex/rust-v0.162.1/codex-rs/core/config.schema.json),
[model limits and prices](https://developers.openai.com/api/docs/models/gpt-6-luna).

## Bounds and the unchanged $2 mission

These are computed bounds, not measured reservations. Context remains unknown
before dispatch, so it uses the entire published model envelope.

| Role / model | Output maximum | Bound USD | Sub-cap USD | Settled-spend room USD |
| --- | ---: | ---: | ---: | ---: |
| Lead / Luna | 128,000 (model maximum) | 0.306000 | 0.40 | 0.094000 |
| Writer / Haiku | 32,000 (CLI control) | 0.665500 | 0.80 | 0.134500 |
| Reviewer / Luna | 128,000 (model maximum) | 0.306000 | 0.40 | 0.094000 |
| Canary and repair | Per role/model | Per invocation | 0.40 | Depends on model |
| Lead/Research/TestProposal/Reviewer / Haiku | 8,000 (CLI control) | 0.599500 | No separate mission allocation yet | Not dispatched |

Lead + Writer + Reviewer + retained canary/repair allowance =
$0.40 + $0.80 + $0.40 + $0.40 = **$2.00**. Each of the first three has
positive spend room for several tiny settled turns. A real `Budget::can_admit`
regression admits three $0.005 settled turns per role with its bound retained.
This is a ledger proof, not a claim that a live mission already ran.

Aggregate purpose sub-caps are durable and independent of worker identity.
The regression initially admitted a new worker above the Worker purpose cap;
the shared budget now rejects that bypass. Production dispatch derives the
purpose and output bound from the saved role rather than caller claims.
It also generates its own fresh session key because the actual role constructors
launch without resume. A regression demonstrated that a caller's old $0.05 base
discounted a fresh $0.06 invocation to $0.01. The corrected dispatcher prices
the entire $0.06 and cannot borrow that base or the caller's purpose.
Canary/repair may use unused funds by an explicit reallocation inside $2.00;
Haiku's unknown-context bound does not fit the initial $0.40 reserve alone.

## Evidence and scope

Private root: `C:/Users/Erick/AppData/Local/Faber/t13-exposure-20261014/`.

| Evidence | Result |
| --- | --- |
| `r120-preflight/qualification-preflight.json` and generated protocol | Same installed versions/hashes; no model turn |
| `r120-output-writer/live-denial.json` | 67 output tokens; $0.0001475; marker unchanged |
| `r120-output-control/live-denial.json` | 2,048 tokens; $0.0016747; recovery gap |
| `r120-output-control-no-retry/live-denial.json` | 2,048 tokens; $0.0015732; network retry control does not fix recovery |
| `r120-output-control-budget/live-denial.json` | One 512-token request; $0.0003711; terminal budget stop |
| `r120-output-control-final/live-denial.json` | One 512-token request; $0.0003710; whole-counter verification passes |
| `validation-r120-bounds/proofs.json` and `proof-01.log` through `proof-07.log` | Required local proofs; consult completed exit codes |
| `r120-red-session-base-runtime.log` / `r120-green-session-base.log` | Fresh-cost undercharge reproduced and fixed |
| `r120-red-extra-requests.log` / `r120-green-bounds-policy.log` | Missing control reproduced; 49 budget and 21 policy tests pass |
| `r120-green-helper-final.log` | Ten diagnostic self-checks pass |
| `r120-quota-evidence.json` | R88 headroom zero; source/baseline/calibration evidence retained |
| `validation-r120-final/proofs.json` | Required final rerun after the remaining corrections |
| `r120-timeout-projects-recheck.log` / `r120-timeout-remote-recheck.log` | Unchanged-code focused repeats pass at 207.1 MB and 34.8 seconds |
| `validation-r120-rechecked/proofs.json` | Full rerun after those timeouts; completed exits are authoritative |

The five settled R120 diagnostics sum to **$0.0041375** API-equivalent cost.
They used first-party subscription billing, with paid fallback explicitly
false. This subtotal excludes older unsettled diagnostic turns; those cannot
be presented as measured zero. No mission cost, installed candidate, real
canary or positive Resume is claimed here.

The initial full rerun failed only Projects RAM. The
[offscreen-rendering correction](2026-10-14-projects-offscreen-memory.md)
reproduced that failure, fixed the rendering cause and then passed all 1,392
checks with a 215.3 MB Projects peak. That coherent correction was committed
and pushed as e159c02; CI was not awaited between steps.
The subsequent full run passed 1,391 checks but timed out waiting for Projects
after a reload and for the local git probe. Both isolated repeats passed with
unchanged code. Other hidden suites were running; a late resource reading did
not establish the cause. Deadlines were not enlarged and failures were not
removed. The full rerun is retained separately.
That rerun completed with all seven required commands at exit 0 and **1,393
tests passed, zero failed, zero not run and zero skipped**. Projects peaked at
**221.9 MB** and the local git scenario completed in **35.8 seconds**. The last
scripted update scenario exercised the full 300-second probation. These are
local proof results, not a successful real mission or positive qualified Resume.

The owner confirms that the orchestrator installed v0.3.1 into `E:/Apps/Faber`
at approximately 15:05 UTC. Current `faber.exe` SHA-256 is
`e6fd76f5eedf9ad7259fb60e8a586d07edf8283a3d13f2ffa92cb2bbd519b369`.
That owner-authorized change is the next mission's stable-install baseline,
not a Task 13 mission effect. Owner checkout byte identity must be established
across the actual mission window, excluding unrelated earlier owner commits.

# Task 13 continuation: green CI, uncached bound controls, context-only probe

Observed on 2026-10-14 after the orchestrating session restarted. The pushed
`b942eef184adf901dbe6eb9b0320b076a2b1cbf9` has successful Windows and macOS
[CI](https://github.com/startRED/faber/actions/runs/38572893418). PR #19 remains
a draft. The new cross-vendor review of `d66c472` and `b942eef` has not arrived.

## Bound correctness

R117 prices the interrupted request's input uncached. Claude's default cache
creation may cost more than uncached input, so that calculation requires cache
creation to be disabled. The new regression first failed with the shared role
builder returning no `DISABLE_PROMPT_CACHING` value. Every role's start and
resume now sets the documented `DISABLE_PROMPT_CACHING=1` in its fixed settings.
The output maximum remains the model's full 128K; no smaller universal limit is
claimed. Manual Chat/Projects keep their existing settings.

The official [environment reference](https://code.claude.com/docs/en/env-vars)
documents this control for all models. This continuation proves the shared
arguments and exercises startup/control requests, not a new streamed model
turn proving zero cache creation counters. Admission remains closed.

## Context probe without a model turn

The installed Claude Code 2.1.296 accepted the official Agent SDK's initialize
and `get_context_usage` controls on an owned hidden desktop, before any user
message. There was no assistant stream, result or model prompt. Only counts
were retained; no memory-file contents or integration configuration was read
or saved by the helper.

The response reported **253 tokens**, comprising 243 system-prompt tokens and
10 tool-category tokens, with no loaded memory files or MCP tools. The reported
window was 1,000,000; the free and compaction-buffer categories are not used
context. The controlled job ended in **13 ms**; probe elapsed time was 2571 ms.
The outside marker's bytes and SHA-256 matched after job termination.

For comparison, the formula using only that already-built context and the
128K output maximum is **$0.064026, computed**, not measured spending. It is
**not the bound of an upcoming invocation**: the next user prompt is absent.
No reservation, authoritative receipt, process-tree writer admission or public
Resume was opened by this response. A parser regression rejects missing,
negative, zero, over-window or wrong-model counts and inherited inventories.

The [SDK reference](https://code.claude.com/docs/en/agent-sdk/typescript#sdkcontrolgetcontextusageresponse)
documents full counting through token-count API calls outside the message
stream, and summary counting through local estimates. The default full count
was requested, with the exact control form checked against the official Python
SDK source. This probe establishes that the installed CLI supports the call;
it does not certify a complete next-request census or invent a token allowance
for additions the CLI may make after that call.

## Codex controls and remaining stop

The version-matched schema's `features.rollout_budget` is not a smaller
per-request output maximum. Its implementation updates the consumed budget
from completed-response usage; its exhaustion test waits for completion before
reporting the budget error. `model_context_window` describes the window and
`features.tool_registry` controls collisions/metadata, not tool removal or a
hard input/output limit. No guessed controls were added.

Sources: [configuration reference](https://developers.openai.com/codex/config-reference/),
[version-matched rollout accounting](https://github.com/openai/codex/blob/rust-v0.162.1/codex-rs/core/src/rollout_budget.rs),
[completion/exhaustion tests](https://github.com/openai/codex/blob/rust-v0.162.1/codex-rs/core/tests/suite/rollout_budget.rs).

The production dispatcher still has no trusted complete pre-request context
census. Its unchanged fallback bounds are **$0.306000 for Luna** and
**$0.820000 for Haiku**. Lead's $0.30 and Writer's $0.60 reject these in the
actual budget-admission regression. Reviewer's $0.40 covers Luna's bound.
One of each totals $1.432000, so the total $2 cap alone is not the demonstrated
failure; the authorized role split is. No cap or role allocation was changed.

This spending-admission failure remains the stop. Fresh model turns, receipt
opening, writer integration, the real miniature canary, fresh-clone mission,
install, rollback and positive public Resume were not executed. G1-G10 and
AS-1..AS-9 retain the statuses in the main acceptance report.

## Private evidence and preservation

Root: `C:/Users/Erick/AppData/Local/Faber/t13-exposure-20261014/`.

- `delivery-r117.json`: clean pushed b942eef, draft PR and both CI jobs green.
- `r117-owner-after-ci.json`: read-only comparison against the original 291
  nonprotected tracked paths and metadata. Owner checkout is **not unchanged**:
  `docs/design.md` and index differ, with separate owner commit `913fdf9` and two
  new protected skill paths. Only those path names were listed, never opened.
  Attribution is unknown. Both `E:/Apps/Faber` install hashes still match.
- `r117-continuation-preflight/qualification-preflight.json`: fresh CLI versions,
  exact binary hashes and subscription account metadata, zero model prompts.
  Claude's paid-fallback status remains unreported; Codex reports false. The
  executable hashes match the earlier observations; no other AppData/vendor
  installation files were opened.
- `r117-context-census/live-denial.json`: immutable context-only observation,
  zero model turns, token-category counts, timings and marker hashes.
- `validation-r117-context-caching/proofs.json`: the seven required checks;
  command logs `proof-01.log` through `proof-07.log`. Final counts and the pushed
  follow-up CI are recorded after completion.

All seven required local commands exit 0. The complete suite reports **1390
passed, 0 failed, 0 not run, 0 skipped**. Scripted Projects peak is **235.4 MB**,
under the 250 MB test ceiling. Four owned processes ended in each stop/drop/crash
scenario. Full fixture probation and rollback remained hidden. The
[reduced evidence](2026-10-14-self-development-context-census.json) retains exact
command logs and does not turn scripted measurements into real-run evidence.

Nine qualification-helper tests pass separately. The real mission's cost and
RAM remain unknown; no model turn was requested in this continuation. The
context-only number is never added to the historical measured subtotal. No
owned evidence was deleted and no deletion was refused.

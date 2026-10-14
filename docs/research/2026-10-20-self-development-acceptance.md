# Task 13: R117 bounds implemented; authorized role caps reject admission

Observed on 2026-10-14 in `selfdev-t13`, draft PR #19 into `selfdev`.
Task 13 and R96 remain incomplete. R117 supersedes the interrupted-reserve route
in [the archived metering report](2026-10-14-self-development-interrupted-metering-stop.md).
No measured overshoot reserve or invented safety factor is used.

## Result and stop

The latest cross-vendor review of `8a62018` is accepted: checks 1, 3, 4 and 5 are
sound; check 2 identified a real gap. Before the fix, fake-wire tests of both
actual role loops timed out on unknown permission requests. Known command/file
approvals now receive `decision: decline`; permission approvals get an empty
permission grant scoped to the turn; MCP elicitation gets `action: decline`.
Claude's `can_use_tool` gets `behavior: deny`. Every other server request ends
the owned role immediately with fatal `reason.project.restrictionsUnproven`.
Future names ending in `requestApproval` do not get a guessed response shape.
Manual sessions keep their existing handling. No finding in this latest review
is rejected.

The coordinator also propagates fatal role failures even while cost is unsettled;
R117's nonterminal accounting cannot swallow a security failure. Its regression
first returned `Ok(())` instead of the restrictions key. Other fatal diagnostics
map to the fixed invocation error key. Accounting still requires an owned-job
retirement proof before it marks termination; an error alone never releases it.

R117 charges an interrupted invocation without settled counters at a **computed
bound**: last settled usage plus one uncached request at the current context and
the documented output maximum. Overflow and unqualified model IDs fail closed.
Unknown CLI-built context uses the full documented window; an earlier turn's
input count is not a census of the next request. The production dispatcher
replaces caller exposure with this conservative calculation before spending
admission. Public role admission stays closed; a parameter cannot unlock it.

The ledger marks bound attribution separately from measured cost, charges the
terminated invocation once, retains the charge across restart and late results,
and excludes it from calibration. Late cumulative Claude results subtract the
saved session base. An interrupted/bounded session cannot reuse the allowance.
Incomplete nonterminal accounting does not itself trigger a budget interrupt;
owner/deadline/policy stops remain available. Spend above the admitted amount
records the larger charge and closes the driver.

A new live Claude interruption also lacked settled final counters: its error
result had zero token categories despite earlier streaming usage. It acknowledged
Stop in **7 ms**, emitted zero further text deltas, and closed the owned job in
**17 ms**. These observations are not a monetary reserve. Its tools/MCP/skills
were empty, plugins were builtin-only metadata, and marker bytes/hash matched.
No new Codex interruption or model turn was requested under R117.

The new stop is **`reason.budget.cap` for the runbook's authorized role split**.
There is no trusted pre-dispatch census of the CLI's complete prompt yet. The
resulting conservative bounds exceed Lead's $0.30 and Writer's $0.60. Regression
tests exercise the actual `Budget::can_admit` and require both rejections; a
certified 50K context fits those same tests. No cap was raised or repartitioned,
and no real mission was dispatched.

## Supported controls and dollars

Codex's installed executable generated its JSON schemas on an owned hidden
desktop. `ThreadStartParams` and `TurnStartParams` expose no output-token maximum;
the [version-matched config schema](https://raw.githubusercontent.com/openai/codex/rust-v0.162.1/codex-rs/core/config.schema.json)
has none either. No key was guessed. The new project start/resume override pins
the supported `service_tier: default`, matching standard prices. That override
has regression proof, not a new live attestation. Luna's documented output
maximum is 128,000 tokens including reasoning; its context envelope is 1,050,000.
Above 272K input, standard prices are $0.20 input / $0.75 output per million.
[Model limits and prices](https://developers.openai.com/api/docs/models/gpt-6-luna).

Claude's supported `CLAUDE_CODE_MAX_OUTPUT_TOKENS` is set to `128000` in shared
project settings for all roles/resumes. Documentation says "most requests", so
it is not claimed as a universal smaller cap. Exceptions remain covered by the
model's 128K maximum. Haiku has a 1M context envelope; above 100K input it costs
$0.50 input / $2.50 output per million; smaller context costs $0.10 / $0.50.
Sources: [supported control](https://code.claude.com/docs/en/env-vars),
[model limits](https://platform.claude.com/docs/en/models/overview),
[prices](https://platform.claude.com/docs/en/about-claude/pricing).

Every amount below is a **bound**, initially with zero last-settled spend.
Additional settled spend is added. Diagnostic effort was low; curated mission
profiles use medium for these models. The maximum output covers both.

| Role / model | Pre-dispatch context | Bound USD | Authorized cap USD | Result |
| --- | --- | ---: | ---: | --- |
| Lead / `gpt-6-luna` | Unknown; 1,050,000 envelope | 0.306000 | 0.30 | Reject |
| Writer / `claude-haiku-5-5` | Unknown; 1,000,000 envelope | 0.820000 | 0.60 | Reject |
| Reviewer / `gpt-6-luna` | Unknown; 1,050,000 envelope | 0.306000 | 0.40 | Fits this cap; role admission closed |
| Canary/repair using Luna | Same unknown envelope | 0.306000 per request | 0.70 combined | Two fit; no canary ran |
| Canary/repair using Haiku | Same unknown envelope | 0.820000 per request | 0.70 combined | Reject |

One Lead + Writer + Reviewer sums to **$1.432000**, below $2.00. Holding all three
at once plus the untouched $0.70 canary/repair buffer needs **$2.132000**. Settled
sequential turns can release unused reservations; that concurrent sum does not
mean every $2 mission is impossible. The authorized split still fails before a
useful mission starts: Lead is $0.006 over its cap and Writer is $0.22 over.

For comparison only, a certified 50K context gives $0.069000 for either model.
Claude's observed streaming input here is 966 tokens, which would calculate to
$0.064097, rounded to microdollars. That request was already in flight; it is not
pre-dispatch evidence for the next turn. The immutable manual receipt uses the
more conservative $0.820000 envelope. Historical unknown turns are not given
invented token counts or retrospectively labeled measured.

## Installed identity and boundaries

| Vendor | Fresh version | Exact resolved executable SHA-256 |
| --- | --- | --- |
| Claude Code | 2.1.296 | `76de74f7694a5bdc03378a5f837c9e132827bf004c98c276bc4e9094ff06769a` |
| Codex | 0.162.1 | `dd13bdb162a184fa04039488b234a8258ef9b76f84e926c505d5742b19249a6a` |

Fresh preflight reports subscription billing for both and paid fallback false
for Codex. Claude's interruption has subscription metadata but no fresh
overage/paid-fallback attestation. No paid use is observed or claimed impossible.
Only exact resolved programs were hashed under R112. User CLI configuration
and credential files were never directly opened.

The same hashes preserve the scope of earlier G1 observations: actual Codex
inside/outside native patches received decline and kept baseline bytes. All
five roles start/resume read-only with on-request approval. Claude grants no
Edit/Write/NotebookEdit/Bash. Both reported zero inherited tools/active MCP
servers. Codex's four disabled entries each expose zero tools/resources/templates,
with empty hooks/plugins. Claude hooks inventory remains unreported. Marker
readback proves unchanged bytes and no disclosure, not an OS audit of all reads.

## Evidence and validation

Private root: `C:/Users/Erick/AppData/Local/Faber/t13-exposure-20261014/`.

- `r117-preflight/qualification-preflight.json`: fresh hashes/versions/accounts;
  zero model turns. `r117-preflight/codex-protocol/`: installed schemas, including
  request parameters and verified permission/elicitation decline forms.
- `r117-claude-interrupt/live-denial.json`: one real interrupt, loadout, streamed
  partial counters, zero settled counters, computed bound, timings and marker
  hashes; informational only, never an admission receipt.
- `r117-owner-before.json` and `r117-owner-after-probe.json`: all 291 tracked
  owner files, HEAD/index/packed-refs and two install files match. Untracked
  files and build outputs are excluded.
- `r117-owner-after-validation.json`: final comparison is **not unchanged**.
  The owner checkout now has separate commit `913fdf9`, changed `docs/design.md`
  and its index, and added `.claude/skills/faber-ui/SKILL.md` and
  `.claude/skills/second-opinion/SKILL.md`. The original auditor refused before
  opening either protected path. A baseline-only read-only rehash records names
  of added paths without opening them. Attribution is unknown; this task did not
  restore or modify owner files. Both stable install hashes still match.
- `validation-r117-coordinator-final/proofs.json` and `proof-01.log` through `proof-07.log`: all
  seven required commands; final counts are inserted after the suite completes.
- Earlier byte/marker proofs: `g1-codex-role-tools`, `g1-codex-native`,
  `g1-claude-native`. Earlier unmetered Codex observations: `g2-codex-stop` and
  `g2-codex-stop-drained`.

Regression-first failures reproduced both unknown-request timeouts, missing
output setting/calculation, malformed bound records freeing accounting and late
cumulative double-charging. Production parsers/ledger and fixtures cover both
vendors. The unit-only authorized Resume seam accepting an accounted bound is
not a valid-receipt positive public Resume proof.

The immutable Claude observation predates the final helper adjustment. Its
historical `reservationMicros: 30000` field was the CLI's estimated cap, not a
proven hard bound. The helper now records the computed reservation separately
from that CLI estimate and no longer performs a stream-cost budget interruption.
No further live turn was requested after cap rejection.

## Cost and RAM

The real acceptance mission did not run; its cost and RAM are unknown. This
continuation requested **one** tiny Claude turn; its envelope is **$0.820000,
computed**, with settled cost unknown. Partial streaming usage priced to
$0.000197, which is not the final charge. Live CLI peak RAM was not sampled.
The previous combined settled diagnostic subtotal remains **$0.01299530**;
four earlier unknown turns plus this one leave historical total spend unknown.
These are subscription API-equivalent accounts, not a cash API invoice.

All seven required local commands exit 0. The final suite has **1389 passed,
0 failed, 0 not run, 0 skipped**, in **1203.267 seconds**; the eight separate
qualification-helper checks also pass. Scripted Projects peak is **219.2 MB**,
idle full/reduced-motion is **114/112 MB**, and warm fake guard/whole tree is
**1.1/142.6 MB**. These are scripted measurements, never real-mission RAM.
The [reduced evidence](2026-10-14-self-development-r117.json) records the raw
command log paths, calculated bounds, interruption metadata and accurate audit.

## Gates

| Gate | Status | Evidence / remaining |
| --- | --- | --- |
| G1 Authority | Not passed; partial native proof | Native denial/bytes and unknown-request stops pass. Final owner comparison changed with separate commit 913fdf9; receipt/writer integration pending. |
| G2 Spending | Not passed; cap rejection | R117 calculated accounting tested; Lead/Writer exceed authorized subtotals. No reserve measurement required. |
| G3 Scheduling | Not passed | No admitted live graph. |
| G4 Acceptance | Not passed | Real fixture subprocess red/green proofs; no real candidate/review. |
| G5 Recovery | Partial, scripted | Fixture recovery and once-only bound reconciliation; real rollback/public positive Resume pending. |
| G6 Self-update | Not passed | Case 4 copies a prebuilt launcher/injects CanaryVerdict::Pass; actual build/miniature mission pending. |
| G7 Resources | Partial, scripted | Hidden screen/RAM suite; live lifecycle absent. |
| G8 Real use | Not passed | No fresh Seguro mission. |
| G9 Kits | Partial | Zero inherited tools/active servers; Claude hook inventory and complete receipt absent. |
| G10 Software bar | Not passed | No real software-bar acceptance. |
| AS-1 Motion | Passed, scripted | Hidden frame/reduced-motion checks. |
| AS-2 Composer | Passed, scripted | Hidden growth/cap/scroll/shrink checks. |
| AS-3 Breadcrumbs | Passed, scripted | Hidden five-locale checks; no new native-speaker review. |
| AS-4 Gap | Passed, scripted | Hidden 64 px checks. |
| AS-5 Windows | Partial | Owned hidden jobs; complete real mission/update audit absent. |
| AS-6 Disk | Not passed | No real promotion/cleanup; evidence retained, no refused deletion. |
| AS-7 Specific plan | Not passed | No admitted real plan. |
| AS-8 Dual vendors | Not passed | Code reviews/native probes are not a fresh candidate-review receipt. |
| AS-9 Questions | Not passed | No real lead question log. |

Earlier seven-point review disposition stays in the archived report. Proof
dispatch, marker readback and the dirty-owner baseline were addressed. Reserved
scalar `unknown` is documented and not a defect for validated numeric/git-ID
metrics. Audit `unchanged` compares to its already-dirty baseline, not git HEAD.

## Remaining

Establish a trusted pre-dispatch context census or a universal smaller output
bound fitting the unchanged caps. Then implement private fresh version/hash
receipt consumption, integrate the Windows writer process proof, run the real
miniature canary and authorized Seguro mission with fresh other-vendor review.
Build/install only in the sandbox, prove rollback/positive exactly-once Resume
and measure real cost/RAM. The fresh clone, sandbox install and promotion were
not created. PR #19 stays draft; pushed-head CI is recorded separately.

# Task 13: self-development acceptance

## T13 final result (2026-10-15 UTC): a real coordinator mission lands and a real candidate is built; the real canary waits, so promotion, probation and rollback are not reached

Decision R135 (`faber-notas/decisoes.md`) set the rules for this run. Claude Code 2.1.296 and Codex 0.162.1, hidden, no
visible window, no shell for any child process. The owner's checkout, `E:\Apps\Faber` and the owner's running app were
not touched.

**Passed for real**

- **A production coordinator.** `Coordinator` (engine) reads each role's kept reply and drives Lead, Writer, red proof,
  green proof, other-vendor Reviewer and landing through the journal. One real Faber goal ran in Seguro mode on a clone
  of a baseline that carries a staged regression: `settle()` in `engine/src/usage.rs` added a resumed turn's trusted
  total instead of subtracting it. The Lead (Claude Haiku) planned one writer task, the Writer (Claude Haiku) patched
  the line on a lease, the engine ran `usage::tests::a_resumed_turn_costs_the_difference` red on the base and green on
  the exact tree, a fresh Codex Luna Reviewer passed that tree, and the engine landed it on `faber/<mission>` with the
  owner branch untouched. Sixteen landed missions are recorded; each cost **US$0.0050 to 0.0059** API-equivalent
  (for example Haiku 1,015 + 3,561 and Luna 1,211 millionths). Their ledgers sum to US$0.0906; with the interrupted and
  refused attempts the whole run stayed well under US$0.20 (cap US$2.00).
- **Positive Resume, once.** The harness refuses the first Writer start with a Wait, then the product path
  (`Runner::load(..).resume(..)`) moves the mission Paused to Running with grant revision 3 unchanged and no invocation
  added (`lead-1` before and after); a new coordinator then continues. Seen in every landed run.
- **A real candidate was staged by the running baseline.** The baseline (slot A, started by the trusted launcher on a
  hidden desktop) found the landing, froze the exact tree, built the candidate in about 6 minutes (release, offline,
  locked), and its saved-data round trip with both the old and the new binary passed. It reached `canaryPending` at
  about 412 s.
- **The real canary child ran and refused to pass.** The candidate's own `--faber-local-canary` ran in real mode and
  answered `reason.budget.calibration` (a wait, not a failure). Nothing was waived.

**Blocked, and exactly why**

- **Real canary admission.** Production admission (R131/R132, the waiver is testkit-only and single-mission) needs a
  measured weekly conversion or a bootstrap margin of two points under the day line. Claude's weekly meter read 0.44
  against a day line of 3/7 = 0.4286 (negative margin, because the shared account is used heavily). A mission that costs
  US$0.006 never moves a 1% weekly meter: in ten reruns no Claude seven-day tick happened, so no conversion was learned.
  The line moves to 4/7 at 2026-10-16 07:59:59 UTC (margin about 0.13), but the reused qualification receipts expire at
  about 03:15 UTC, so the run needs fresh receipts first. The canary waits and the build stays ready; it was never
  forced to pass and the offline replay never counts as a pass.
- **Not reached, so not claimed:** promotion into slot B, the accepted revision being the running one, the 300 s
  probation, and the bounded rollback drill (`drill` in `promote.rs` runs only after a Healthy promotion, and it uses a
  crashing candidate with an injected verdict, so it proves recovery only). The fixture tests for these stay
  "simulated".

**Defects the real run found and fixed** (a regression test each unless noted)

- A real mission's grant did not sum to its ceiling and the canary share overflowed; `mission_grant` splits it so each
  role's largest turn is admitted.
- `calibrate_mission` failed on a reading stamped in the same second; it now waits out the stamp.
- The saved-data probe read the engine's `budget.json` (a ledger) as a bare budget and refused every real data folder
  with `error.update.invalidState`; it now round-trips the ledger.
- The slot's environment lacked the profile and toolchain variables a real candidate needs.
- The worker contract named `ping` but not its values; a Lead answered `"ping":0` and the envelope was refused as
  `error.wire.malformed`. The contract now lists the six strings (test keeps them in step with the enum).
- The Reviewer dropped the required `next` field of a complete action (also `error.wire.malformed`); the brief and the
  contract now say to keep it. Found by capturing the raw refused text (a temporary debug line, removed).
- The native build writes `src-tauri/gen` into the frozen source, so the "build modified accepted source" check always
  failed on a real build; that folder is now treated like `dist` and `node_modules`, and only the exact folder is
  exempt (test).
- The canary fixtures are compared byte for byte, but a baseline built from a checkout that converts line endings
  carries CRLF copies and the candidate (built from git blobs) LF ones: "canary fixture changed". `.gitattributes` now
  pins them as `-text`.
- Provider readings of a rolling Codex window can leave `first.read_at <= start` for one second at 0% usage; the
  harness retries with a fresh data folder (a product-side wait is the same outcome).

**Cost and RAM.** Real spend: about US$0.12 API-equivalent in total, all on the subscriptions. The hidden engine worker
idles at 3 to 5 MB; with a CLI turn running the tree is 83 to 160 MB. While the engine's own proof compiles the clone,
the whole process tree peaks at 1.3 to 1.6 GB (rustc and cargo, counted in the tree). The full app with its window
and the baseline slot were not sampled in a real run.

Timings of one run: Lead about 100 s, positive Resume about 170 to 220 s, Writer and proofs to about 380 to 420 s, landing
and calibration at about 400 s; candidate build about 6 min after that.

**Waiting for owner away.** The promotion, probation and rollback steps start the candidate app, so they are parked until the
owner is away from the screen; the screen suites were not run locally for the last commits (CI runs them).

**What needs the owner.** Fresh qualification receipts and a new run after 2026-10-16 07:59:59 UTC (or a day when
Claude's weekly meter has room), then the promotion, probation and rollback gates can be observed. Nothing in the
safety rules was loosened to get here.

---

# Earlier result (superseded by the final result above)

## T13 takeover result (2026-10-15 UTC): receipts and a real seam mission pass; canary, install, rollback and Resume are blocked

Decisions R133 and R134 (`faber-notas/decisoes.md`) set the rules for this run. Everything below was observed on the
installed Claude Code 2.1.296 and Codex 0.162.1 binaries, hidden, with no visible window and no shell.

**What passed for real**

- **Qualification receipts.** Fresh live probes (`t13-run1/`, rows under 6 hours old) issued `claude.json` and
  `codex.json` with all five checks true. Claude's hooks are off by launch flags (`--safe-mode`,
  `--setting-sources ""`, `disableAllHooks`), recorded in the receipt, and no managed policy exists. Codex's one
  instruction source is the user's global file, recorded by path and SHA-256. A launch without the Claude flags and
  any other Codex source both fail in tests.
- **A tiny mission through the engine's own role machinery** (`--live-seam`, a throwaway repository, R132 waiver
  single-use with its marker). Claude Haiku wrote `src/add.js` as a patch envelope on a lease; the engine applied it,
  ran `check.js` itself (red exit 1 at the base, green exit 0 on the exact tree), a fresh Codex Luna session reviewed
  the exact candidate and passed it, and the engine landed tree `7a2d5672...` on `faber/<mission>` with the owner
  branch untouched and `report.json` written. Both invocations settled: Haiku 594 and Luna 780 millionths, **US$0.001374**
  API-equivalent on the subscriptions; `calibrate_mission` returned Ready. The Writer's owned process tree was
  retired by `worker_termination` and its lease settled before any broker IO.
- **RAM.** The hidden harness tree (engine plus the live CLI) idled at 2 MB and peaked at **125 MB** with the Claude
  Writer running (29 MB with the Codex Reviewer). This is not the full app with its window.

**What this is not.** No production coordinator exists: nothing reads a worker reply and drives the next step. The
harness (`testkit/examples/project-qualification/seam.rs`) plays the Lead and the coordinator, scripted; every model
turn, spend, lease, proof, tree and landing goes through the engine calls a coordinator would make. The run is a
proof of the seam, not an autonomous mission.

**What is blocked, and why**

- **Canary.** `probe_canary` returns `CanaryVerdict::NotRun` by design, and no production code produces `Pass`.
  Building and running a candidate that runs its own miniature mission needs the coordinator first.
- **Install, rollback, positive Resume.** All three need a canary pass on a built candidate. The existing tests that
  cover them copy a prebuilt launcher and inject `CanaryVerdict::Pass`; they stay "simulated".

**Defects found and fixed on the way** (each with a regression test)

- A Writer's own session close produced a fatal "stopped" event that paused the whole mission after a good turn.
- No role could end except a Writer: read-only roles never settled. `Engine::finish_role` now does it.
- Replies were discarded. An accepted envelope and the key of a refused turn are now kept in the journal.
- The worker contract had no envelope grammar and the header showed `lease: null`; both roles invented keys until
  the contract named the exact keys. Haiku and Luna then answered in the strict format.
- The Codex stop probe demanded zero bytes of text in flight at the moment of interrupt, a race the pipe cannot
  win; the skills question was answered by a skill name in the global instruction file. Both are corrected in R134.

**Flakes seen, not hidden.** A provider reading stamped to the whole second can be "from the future" for a tick;
the harness waits 2 seconds. Two Codex native probes failed before the skills question was fixed (a 20-second
model turn deadline, then the skills answer); sixteen seam attempts failed on the points above before the final
one passed. Their ledgers are kept under `aborted-attempts/`; all seventeen ledgers sum to US$0.0131 (Claude
0.0086, Codex 0.0046). The cap of US$2.00 was never approached.


## Earlier result: R132 continuation (2026-10-15 UTC, superseded by the takeover result above)

The owner's R132 exception removes the quota-line obstacle for one US$2.00 mission; it does not waive boundary
qualification. The fresh zero-turn readings are Claude **36%** and Codex **44%** weekly, both subscription billing
with paid fallback **false**. The unchanged product correctly refuses R131 admission at the current **28.5714%**
line. These are actual provider counters; Codex's reported 1–2 hour lag prevents treating a short-window change as
the mission's isolated consumption. No model turn, mission, grant consumption, calibration stretch or baseline was
invented. R132 is **not implemented or consumed**: execution stopped at the prerequisite qualification, as the
owner explicitly required when a probe cannot be proven.

Two runtime defects in the retained Codex work were corrected regression-first:

- The startup catalog omitted the required instruction template. The installed CLI exited before initialization;
  its version-matched catalog parser requires either the legacy base instructions or
  `model_messages.instructions_template`. An original empty template now lets the catalog load without adding
  instructions or native tools. The regression failed with `Null` versus `""`, then all 22 policy tests passed.
- The probe compared Windows paths as strings, so the same workspace with `/` versus `\\` failed acknowledgement.
  Native path comparison fixes the workspace and hook/skill inventory checks; a different workspace still fails.
  The regression failed first, and all 16 qualification-helper self-checks now pass.

The resulting actual Codex session acknowledges the pinned model/provider/effort, read-only sandbox, no network,
on-request approvals and the owned workspace. It still reports **one instruction source**, so the strict empty
instruction inventory fails before complete loadout qualification. The harness did not open or copy any
instruction-source file.

Claude's SDK initialization and context-only controls completed without a model turn. They report **zero MCP
tools**, **zero memory files** and a pre-prompt context count of **253**, but expose no effective hooks census.
That count is not the next request's complete context. The documented safe mode and non-managed
`disableAllHooks` still permit managed-policy hooks; absence of a hooks field cannot prove an empty inventory.
See the official [safe-mode contract](https://code.claude.com/docs/en/cli-reference#cli-flags) and
[disableAllHooks scope](https://code.claude.com/docs/en/settings-reference#disableallhooks).

No receipt was issued for either vendor. This is a qualification stop, not a claim that a native escape occurred.
The five receipt checks map to evidence as follows; historical observations are not promoted into new passing
receipts for a changed launch configuration.

The intended R120 split remains Lead/Luna US$0.40, Writer/Haiku US$0.80, Reviewer/Luna US$0.40 and
canary/repair US$0.40, totaling US$2.00. The corresponding complete-invocation **computed bounds**, not measured
spend, are US$0.306000, US$0.665500 and US$0.306000. Their remaining spend room is US$0.094000,
US$0.134500 and US$0.094000. These values come from the supported controls and dated prices in the
[R120 bound report](2026-10-14-self-development-role-output-bounds.md); no real mission grant was created.

| Probe | Claude Code | Codex |
| --- | --- | --- |
| loadout | Unproven: effective hooks not reported | Failed strict census: one instruction source reported |
| native | Earlier denial/marker evidence only; not requalified | Earlier denial/marker evidence only; not requalified |
| stop | Earlier interruption observation only | Earlier interruption observation only |
| exposure | R117/R120 calculated bounds and control evidence; no complete receipt | Model-maximum calculated bound; no complete receipt |
| billing | Fresh zero-turn subscription, paid fallback false | Fresh zero-turn subscription, paid fallback false |

Private evidence root: `C:/Users/Erick/AppData/Local/Faber/t13-exposure-20261014/`:
`r132-zero-turn/zero-turn-admission.json`, `r132-codex-experimental/live-denial.json`,
`r132-claude-loadout/live-denial.json`, `r132-catalog-red.log`, `r132-catalog-green.log`,
`r132-workspace-red.log` and `r132-observation-self-checks.log`. The earlier failing startup observations remain
under `r132-codex-loadout*`; none is labeled a passing qualification. Markers were read back unchanged.

The owner reports the stable install is now v0.3.3. Its exact executable was read-only hashed as
`b3e8c4b314b4863f8d87632872d3fd7cfb547a66b278df92e9c144612b960094` (9,380,352 bytes). This supersedes
the historical v0.3.1 baseline below; it is not a mission-window before/after comparison. There was no mission
window, so owner checkout byte-identity is not claimed. The fresh clone, its build target and sandbox install
were not created, and rollback and positive Resume were not attempted. Real-mission cost and RAM remain **not
measured**; this continuation's zero-model probes consumed **US$0.00 model cost**. Historical unsettled turns
remain unknown rather than zero.

All seven required local proofs for this correction exit 0 and are recorded under
`validation-r132-qualification/proofs.json`: **1426 passed, 0 failed, 0 not run, 0 skipped**, plus the 16
qualification-helper self-checks. Hidden idle RAM is 117 MB and Projects peaks at 220.4 MB; both are scripted
measurements. Delivery and final pushed-head CI are recorded separately in `delivery-r132.json`.
The historical sections below retain their original dates, measurements and limitations.

Observed on 2026-10-14 in `selfdev-t13`, draft PR #19 into `selfdev`.
Task 13 and R96 remain incomplete. R117 supersedes the interrupted-reserve route
in [the archived metering report](2026-10-14-self-development-interrupted-metering-stop.md).
No measured overshoot reserve or invented safety factor is used.

The [R120 output-control continuation](2026-10-14-self-development-role-output-bounds.md)
supersedes the historical role-sub-cap stop below. Supported smaller Claude
request controls, a between-request estimate stop plus full overshoot bound,
and aggregate role sub-caps fit inside the unchanged $2.00 mission cap. They
do not yet prove receipt admission, daily quota calibration or the real mission.
R119 authorizes continuing independent fixes without a CI wait between commits.
The owner-installed v0.3.1 is the new stable-install baseline for the actual
mission window; the earlier stable hash change was an owner action.

After the orchestrator restarted, b942eef's Windows and macOS CI were confirmed
green. The [context/caching continuation](2026-10-14-self-development-context-census.md)
records a regression-first correction disabling more-expensive Claude cache
creation for the uncached bound, a fresh zero-model-turn preflight and a supported
context-only count. That count omits the upcoming user prompt and cannot open
admission. The authorized role-cap rejection and gate statuses below remain.

## Continuation while Codex was paused (2026-10-14 evening)

Four commits after `40e3ef9`, none of which ran a real CLI, mission, canary or quota reading:

- **R131 margin.** The uncalibrated-mission rule (first written as R130) now needs the fresh zero-turn reading to be
  at least **2** points under the day's cumulative line, not 5 (`BOOTSTRAP_MARGIN` in
  `engine/src/project/budget.rs`; `r131_*` tests in `engine/tests/project_budget.rs`). The boundary test failed at
  0.408 before the constant changed. Nothing else in the rule moved: fresh, zero-turn, after the boundary, every
  vendor the mission uses, US$2.00 cap, first stretch and baseline recorded from the mission, nothing invented.
- **Codex role loadout census** (Codex's unfinished work, kept): closed single-model catalog file, empty developer
  instructions, more native features disabled, and the live probe now requires explicit hook, skill, plugin and
  feature censuses plus an acknowledged effective policy. A half-written change to `testkit/src/gate.rs` that called
  engine methods which did not exist was set aside and re-done as the owned-tree work below.
- **Receipt admission** (`engine/src/project/qualification.rs`). `policy::admit` is no longer a constant refusal and
  still takes nothing from a caller. It opens only when `<data dir>/qualification/<vendor>.json` is a plain
  regular file (no link, at most 8 KiB, mode 0600 on Unix), strict schema, `schema`/`vendor` match, all five probes
  (`loadout`, `native`, `stop`, `exposure`, `billing`) recorded as passed, issued within 24 hours (an AI default, not
  an owner number) and not in the future, and equal to what the installed program reports now: the exact trimmed
  `--version` line and the SHA-256 of the exact executable. The receipt is read first; the CLI is hashed and asked for
  its version only if a receipt exists. Script launchers (`node script.js`) never qualify. The driver constructors
  re-check the very program they are about to launch (`policy::admit_launch`). Windows only, as before. 15 tests in
  `engine/tests/project_qualification.rs`; targeted mutations of the version, binary, age, future-date, vendor,
  completeness, size, unknown-field, hash-shape and unobserved-CLI checks each made a test fail. The link and
  directory-as-receipt cases run only where links can be created (not on this PC without the privilege); the
  hash function's own test was not mutated.
- **Writer process proof.** `Process::owned_tree()` hands out an `OwnedTree` (root pid plus the microsecond the
  engine launched it, and the kill-on-close job). `OwnedTree::retire()` closes the job, waits until the root is
  reaped and returns `Retired`, the only input of `Termination::from_retired`. Sessions carry the tree
  (`Session::tree()`); `Engine::dispatch_mission_writer` starts a Writer only through a lease that already holds its
  launch intent, refuses a session without a tree, and attaches the tree's identity with `Leases::attach_process`.
  `Engine::worker_termination` retires the tree, settles spending against the role's own terminal event (or its
  computed bound if it reported none) and returns the proof; `Leases::settle_process` then lets the broker touch the
  lease again. The Windows T13 lifecycle probe no longer releases its writer lease with
  `Termination { process: None, .. }`: a real `ping.exe` runs in an engine job, two recorded literals are refused while
  it lives, and landing uses the retirement proof. The tree scenario gained a retire-only case that fails when
  `retire` does not end the tree (mutated and observed failing). Struct-literal `Termination` values still exist for
  fixtures elsewhere; only `from_retired` derives `tree_dead` from a real retirement.

Proofs for these four commits, run on the head that carries them (`cargo t --all` on Windows, target `t13`): `npm run
lint`, `typecheck`, `build`, `cargo fmt --all --check` and both `cargo clippy` commands exit 0, and `cargo t --all`
reports **1426 passed, 0 failed, 0 not run, 0 skipped** (it was 1393 before). A later one-line compile-time check that
the two new async entry points stay `Send` was verified with clippy and the engine tests, not a second full run.

Not done, and why it was left: no tool issues a receipt yet. `qualification::issue` writes one atomically and refuses
an incomplete set, but mapping the live rows to the five probes needs a final passing row set that does not exist
(Claude's paid-fallback attestation is still absent), so no guessed mapping was written. No receipt exists, so
admission is still closed on this PC.

## Current R120 allocation blocker

The fresh zero-model-turn account probe in
`C:/Users/Erick/AppData/Local/Faber/t13-exposure-20261014/r120-quota-preflight/qualification-preflight.json`
reports Codex subscription/Pro, paid fallback false, and weekly use **0.31**.
The legacy observation field is named `usedPercent`, but contains a fraction:
this is **31%**, not 0.31%. Its reset is 2026-10-20 04:58:09 UTC. At the
observation, the R88 cumulative daily line is **2/7 = 28.5714%**. Therefore
current daily headroom is zero even before the one-point rounding guard.
The next line boundary is **2026-10-15 04:58:09 UTC**, or 01:58:09 in the
owner's UTC-3 time zone. Its next cumulative line is 3/7 = 42.8571%; it does
not waive fresh readings, day-start evidence or calibration.

Read-only inspection of Faber's own quota ledger also found no saved day
baselines and zero measured conversion stretches for either vendor. The
current implementation needs eight qualifying non-overlapping stretches per
model/window. Existing token-price diagnostics do not supply that quota
conversion. No baseline, empty day or dollar conversion was manufactured,
and this run did not modify that owner ledger. New role caps do not authorize
bypassing R88. The owner was asked about this separate allocation decision
while independent local validation continued.

No fresh mission clone, mission dispatch, real miniature mission, sandbox
promotion or positive Resume has occurred. The remaining sequence cannot be
represented as passed on scripted results or unissued qualification receipts.

## Current local proofs

The final R120 rerun completed all seven required commands at exit 0:
`validation-r120-rechecked/proofs.json`, in the private evidence root above.
`cargo t --all` passed **1,393 checks**, with **zero failed, zero not run and
zero skipped**. The ten qualification-diagnostic self-checks also pass.
Projects peaked at **221.9 MB**; the local git scenario finished in **35.8 s**.
Its earlier timeout and successful focused repeat remain recorded, with no
deadline increase. The full 300-second scripted update probation also passed.

The [R120 continuation](2026-10-14-self-development-role-output-bounds.md)
records the initial failures, regression-first corrections, supported controls
and spending arithmetic. CI is checked against the pushed final head; the
private `delivery-r120.json` records that later check. No CI result is invented
in advance, and this report does not waive the pending real gates.

## Historical R117 result and stop (superseded sub-caps)

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
| G1 Authority | Passed real | Receipts for both vendors (reused, under 24 h); the production coordinator ran Claude Lead and Writer and a Codex Reviewer under leases with owned process trees; native denial and stop probes pass. |
| G2 Spending | Passed real, one mission at a time | Caps and a split grant admitted under the R132 single-mission waiver (testkit-only); every invocation settled; 16 missions at US$0.005 to 0.006 each, whole run about US$0.12. Normal R131 admission without the waiver was refused for the canary (see G6), correctly. |
| G3 Scheduling | Not passed | One writer task per mission; no scheduler or live graph. |
| G4 Acceptance | Passed real | Engine-run red then green proof on the exact tree, fresh other-vendor review, landing on the mission ref, for a real bug fixed by the mission (`usage::tests::a_resumed_turn_costs_the_difference`). |
| G5 Recovery | Positive Resume passed real; rollback simulated only | Resume consumed no extra invocation and left grant revision 3 unchanged, seen in every run. The bounded rollback drill was not reached (it runs after a Healthy promotion); fixture recovery tests stay simulated. |
| G6 Self-update | Blocked: real canary waits | A real candidate was staged and built by the running baseline and its data round trip passed; the real canary answered `reason.budget.calibration` (Claude weekly 0.44 above the 3/7 day line, no weekly conversion learnable from a US$0.006 mission). Promotion, "accepted revision is the running one", probation and rollback not reached. Never waived; the install and rollback tests that inject the verdict stay simulated. |
| G7 Resources | Partial real | Engine worker idle 3 to 5 MB, 83 to 160 MB with a CLI turn, 1.3 to 1.6 GB tree peak while the proof compiles the clone. The full app with its window and the slot were not sampled in a real run. |
| G8 Real use | Partial real | A real Faber goal in Seguro mode on a clone of a real baseline with a staged regression, started through the coordinator from a harness, not from the product window. |
| G9 Kits | Partial real | Qualification passed for both vendors (Claude hooks by flags, Codex sources by path and hash). Kits were assembled with an empty inventory; no real tool kit ran. |
| G10 Software bar | Not passed | No real software-bar acceptance. |
| AS-1 Motion | Passed, scripted | Hidden frame/reduced-motion checks. |
| AS-2 Composer | Passed, scripted | Hidden growth/cap/scroll/shrink checks. |
| AS-3 Breadcrumbs | Passed, scripted | Hidden five-locale checks; no new native-speaker review. |
| AS-4 Gap | Passed, scripted | Hidden 64 px checks. |
| AS-5 Windows | Partial real | All live work ran in owned hidden jobs with no window, no shell and sanitized environment; no real install or update audit. |
| AS-6 Disk | Not passed | The throwaway worktree and records are retained as evidence; no real promotion or cleanup run. |
| AS-7 Specific plan | Passed real | The Lead (Claude Haiku) wrote the plan: one writer task naming `engine/src/usage.rs`. |
| AS-8 Dual vendors | Passed real for the seam | Writer on Claude Haiku, fresh Reviewer on Codex Luna, each with its own settled receipt. |
| AS-9 Questions | Not passed | The Lead asked nothing on this goal, so no question log was exercised. |

Earlier seven-point review disposition stays in the archived report. Proof
dispatch, marker readback and the dirty-owner baseline were addressed. Reserved
scalar `unknown` is documented and not a defect for validated numeric/git-ID
metrics. Audit `unchanged` compares to its already-dirty baseline, not git HEAD.

## Remaining

Open items, in the order a real run needs them:

1. **The real canary pass.** Needs a day when Claude's weekly meter has room (margin of two points under the day line)
   or a measured weekly conversion, and fresh qualification receipts (24 h). Then promotion, probation, the running
   revision check and the rollback drill can be observed for real; the harness (`--live-promote`) already does them.
2. **A canary that does not depend on a quota tick.** A mission this small cannot move a 1% weekly meter, so a busy
   shared account can keep the real canary waiting for days. The owner decides whether a smaller admission rule for a
   canary-sized spend is acceptable; it was not changed here.
3. PR #19 stays draft; pushed-head CI is recorded in the final report.

Known gaps in this slice, not hidden: `Termination` still has public fields, so fixtures elsewhere can write a literal
(only `from_retired` is derived from a real retirement); the receipt folder is fixed by the first `Engine` a process
creates; a Claude CLI found only as an npm script launcher never qualifies; the macOS build of the new cfg-gated code
was not compiled here (CI will).

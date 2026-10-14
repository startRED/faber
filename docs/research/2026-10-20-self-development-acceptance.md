# Task 13 acceptance: installed loadout failure

Measured **2026-10-14**, America/Fortaleza. The filename follows Task 13's planned
deliverable name; October 20 is not the measurement date. Baseline `d1379f1`, branch
`selfdev-t13`, draft PR #19 into `selfdev`. **Task 13 and R96 remain incomplete.**

## Actual stop, following R112

R112 removed the earlier executable-fingerprinting obstacle. The hidden preflight
successfully resolved, hashed and version-probed both installed executables:

| CLI | Installed version | Executable SHA-256 |
| --- | --- | --- |
| Claude Code | 2.1.296 | `76de74f7694a5bdc03378a5f837c9e132827bf004c98c276bc4e9094ff06769a` |
| Codex | 0.162.1 | `dd13bdb162a184fa04039488b234a8258ef9b76f84e926c505d5742b19249a6a` |

Both official account responses indicate subscription billing. Codex reports paid
fallback disabled; Claude's paid fallback remains unknown. The preflight's
`reason.budget.exposure` is an unqualified diagnostic, not a new observed spending
failure. It was not used to stop the R112 experiment.

At **2026-10-14 04:56:03 UTC**, a separate hidden Codex app-server started an
ephemeral Writer-policy thread with the production argument and thread-parameter
builders. The acknowledged policy was `gpt-6-luna`, `low`, provider `openai`,
approval `never`, workspace-write, network disabled, and both temporary-root
exclusions enabled. The probe substituted advisory instructions for the native
isolation experiment, but sent **no `turn/start`**. Advisory instructions cannot
account for the inherited server metadata.

`hooks/list` reported zero hooks for the disposable workspace. The following
`mcpServerStatus/list` response differed from the expected empty server set:

| Reported server | Reported tool names |
| --- | ---: |
| Roblox_Studio | 0 |
| node_repl | 4 |
| rea | 170 |
| ytlab | 120 |
| **Total** | **294** |

The helper persisted `state: stopped`, `stopRule: 1`, `qualificationPassed: false`,
and the exact failure `session loadout differs from the empty role loadout`.
The owned-job kill and wait took **20 ms**. This is one no-model cleanup observation,
not measured model interruption latency, a spending reserve, or a child/grandchild
termination proof. No inherited tool was invoked. The result proves a reported
loadout mismatch; it does **not** prove an outside write, marker read, git execution,
web request or model-visible tool invocation.

The executed preliminary helper's `reservationMicros: 30000` and
`stopReserveMicros: 150000` fields were planned amounts, not measurements. They
never authorize admission; the delivered no-model helper replaces that estimate
with `measuredStopReserveMicros: null`.

This is the first observed boundary failure in this continuation. Live qualification
and real acceptance stopped here, as the owner requested. No Claude model turn,
native escape attempt, overshoot experiment, admission receipt, canary, real mission,
promotion, rollback drill or successful Resume followed it.

## Evidence and review of pending edits

Authoritative private observations:
`C:/Users/Erick/AppData/Local/Faber/t13-r112-20261014-qualification/`.

- `preflight/qualification-preflight.json`: executable paths, versions, hashes and
  reduced account/quota metadata. Only the resolved program files were opened for
  hashing; no directory or credential file was opened there.
- `loadout/live-denial.json` and `loadout.log`: the actual failed metadata observation
  and nonzero helper exit. The receipt retains server/tool names, not configuration
  values or tool results.
- `executed-source/`: the exact source used for the measurement, its hashes, and the
  executed helper binary hash. Subsequent source cleanup is not presented as a
  second installed-session run.
- `pending-claude-probe.rs`: the predecessor's unfinished Claude model probe,
  preserved by an exact-path move, not deleted or wired into the delivered helper.
- `helper-tests-final.log`: six passing pure helper tests. The first loadout regression
  failed with `E0425` before its comparison existed.
- `proofs.json` and `proof-*.log`: final required command exits and full output.

The [informational JSON summary](2026-10-14-self-development-loadout.json) is not an
admission receipt. Private receipt creation remains non-overwriting.

Review kept the R112 executable hashing and reduced subscription/quota observations.
The pending production `config/read` change was removed: even discarding values
after deserialization would receive configuration that can contain secrets.
`engine/src/codex.rs` and `engine/tests/project_policy.rs` therefore match the
committed baseline. The retained probe obtains only official session metadata.
Unfinished live model modes were removed; the helper now offers only no-model
preflight and loadout observation. A random `outside-marker.txt` remains a sentinel,
not credentials. Its content was neither prompted to a model nor recovered.

The loadout test rejects an inherited server, a missing server array, and a truncated
page. The marker test checks independent content. Existing tests cover version
validation, fingerprints changing with bytes, permitted executable bytes under a
synthetic vendor folder, and non-overwritable stopped diagnostics. These tests
neither start real CLIs nor open real credential folders.

## Real-run disposition and measurements

The intended real mission remains the runbook's `format_disk_bytes` helper in a
fresh `E:/Documentos/ProjetosIA/faber-t13-target`, in Seguro, with Claude writing and
Codex reviewing. Its $2.00 cap remains unspent: lead $0.30, writer $0.60, reviewer
$0.40, canary/repair $0.70, 120-second maximum turn and planned $0.15 stop reserve.
That planned reserve is not a measured bound.

| Measurement | Actual continuation |
| --- | --- |
| Qualification model turns | 0 |
| Qualification model API-equivalent cost | $0; no model request |
| Real-mission U / R / W / O and settled cost | Not measured; mission not dispatched |
| Stop overshoot and safety-factor reserve | Not measured |
| Real-mission peak RAM | Not measured |
| Scripted Projects board peak RAM | 236.8 MB in this continuation's full hidden suite |
| Warm fake-CLI guard / whole app tree | 1.1 / 141.2 MB, scripted |
| Owned job kill/wait | 20 ms, one no-model observation |
| Time/token savings | Unknown; no paired baseline |

No clone or sandbox install was created. No operation targeted `E:/Apps/Faber`, the
owner checkout source/index, or its `target/debug`; no running owner `faber.exe`
was stopped. Builds use the explicitly prescribed `faber/target/t13`. This continuation
does not claim a fresh whole-folder byte-identity audit of the owner checkout.
The disposable marker workspace and private evidence are retained. No evidence deletion
was attempted or refused.

## Gate table

Earlier scripted passes remain scoped to their fixtures. See the
[October 13 report](2026-10-13-self-development-acceptance.md) for those receipts.

| Gate | Result | Evidence or missing proof |
| --- | --- | --- |
| G1 Authority | **Failed qualification** | Installed role reports four inherited MCP servers; no complete native-boundary proof. |
| G2 Spending | Not passed | Subscription metadata observed; no measured overshoot, reserve or real settled ledger. |
| G3 Scheduling | Not passed | Pure/fixture tests only; no admitted live mission graph. |
| G4 Acceptance | Not passed | Prior fixture proof/review/landing only; no real accepted tree. |
| G5 Recovery | Not passed | Prior fixture rollback only; no live successful Resume. |
| G6 Self-update | Not passed | Production miniature canary remains `NotRun`; no real promotion. |
| G7 Screen/resources | Not passed | Scripted Projects peak 236.8 MB; no complete live UI/RAM evidence. |
| G8 Real use | Not passed | Stopped before real Seguro mission. |
| G9 Kits | **Failed qualification** | Expected empty MCP set differs from installed session metadata; full tools/plugins/hooks attestation absent. |
| G10 Software bar | Not passed | No complete real P/J acceptance receipts. |
| AS-1 Motion | Passed, scripted | Prior rendered intermediate frames and reduced/none checks. |
| AS-2 Composer | Passed, scripted | Prior rendered growth, 40vh cap, scroll and shrink checks. |
| AS-3 Breadcrumbs | Passed, scripted | Prior five-locale selected-locale/separator checks; no native-speaker approval claim. |
| AS-4 Gap | Passed, scripted | Prior rendered 64 px measurements. |
| AS-5 Window audit | Not passed | Hidden launch used; no complete real dual-vendor lifecycle audit. |
| AS-6 Disk | Not passed | No real post-promotion cleanup; evidence workspace retained. |
| AS-7 Specific plan | Not passed | No admitted live helper plan. |
| AS-8 Dual vendors | Not passed | No real other-vendor review or canary. |
| AS-9 Only needed questions | Not passed | No real lead question log. |

## What remains

Resolve the installed MCP loadout mismatch without requesting secret-bearing
configuration or changing the owner's CLI setup. Then repeat full conformance for
both CLIs, including actual native refusal and unreadable sentinel evidence. Measure
several interrupted runs per vendor and the maximum overshoot with a safety factor;
bind `Budget::can_admit` to that engine-owned bound. Consume a private fresh
version/hash receipt, replace the writer refusal with the observed Windows ownership
proof under matching role conformance, and implement the real miniature canary. Only then run the granted clone,
sandbox install, rollback and Resume acceptance.

`policy::admit` remains non-configurable and closed. No manifest, caller Boolean,
budget claim, help flag, fixture pass or informational report can unlock it.

The full hidden suite also exercises the production Windows job with real fake-CLI
child/grandchild processes: stop, drop and owner crash each end all **four owned
processes**. The peer/stale-PID checks pass. See `proof-07.log`, scenario
`projects: engine jobs end grandchildren on stop, drop and owner crash; stale PID
records cannot kill a peer`. This establishes the existing owned-process mechanism;
consuming that proof in writer dispatch remains deferred after the loadout failure.

## Final validation

All seven required commands exit 0 before the evidence commit. `cargo t --all`
reports **1,370 passed, 0 failed, 0 not run, 0 skipped**, in 1,197.076 seconds.
The final pure example run adds **six passing tests**, without real CLI processes.
The full suite repeats the seven T13 fixture checks, including the complete
300-second probation (336.1 seconds for the promotion scenario), early crash recovery
(15.8 seconds) and safe repeated Resume refusal (16.2 seconds). Successful live Resume
remains unproven.

The [informational proof summary](2026-10-14-self-development-proofs.json) records
commands, exits, times and private log paths. PR #19 must remain a draft; the delivery
records the pushed head and its CI results separately. Passing software checks do
not turn the installed loadout failure into accepted self-development.

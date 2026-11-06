# Task 13: native authority repaired; interrupted spending qualification failed

Measured on 2026-10-14, branch `selfdev-t13`, draft PR #19 into `selfdev`.
Task 13 and R96 remain incomplete. This report supersedes the earlier native-write
stop, preserved in [the previous observation](2026-10-14-self-development-native-write-stop.md).

## Current result and stop

The G1 root cause is fixed in the shared Codex project thread builder: all five
roles use `read-only`, `approvalPolicy: on-request`, and `approvals_reviewer: user`
for both start and resume. Faber declines every native approval request. The former
Writer `workspace-write`/`never` pair allowed native `apply_patch` to bypass the
engine's hash-checked patches. The new fake-wire regression first reproduced an
accepted `ToolResult { ok: true }` for a completed native file change; it now
produces a fatal restrictions blocker and stops the owned session. Completed
unknown native tool types also fail closed. Claude uses an empty native tool list
and explicit Edit/Write/NotebookEdit/Bash denials; emitted native tool calls stop
its role loop. Manual Chat/Projects keep their existing thread policies.

The installed Codex genuinely attempted an inside-workspace native patch, requested
approval, received `decline`, and reported `fileChange: declined`. Before and after
bytes are both `baseline\n`. The outside direct attempt also requested approval
and was declined. Neither vendor changed the external disposable file or recovered
the marker. Exact marker hashes are checked again after the owned job ends. This
proves unchanged bytes and no marker disclosure, not an OS audit of every read.
The junction attempt returned an answer without a tool event; no new enforced
junction-denial proof is claimed from that answer alone.

The next spending experiment exposed a real failure of the current metering
strategy. Codex emitted response deltas but no `thread/tokenUsage/updated` before
interruption. It confirmed `turn.status: interrupted`; no usage counters arrived
in the following five-second settlement drain either. The first observation lacked
that drain, so a fresh repeat explicitly checked delayed notification ordering.
Both receipts have unknown final tokens/cost. The drained repeat stopped after
131 response bytes, with zero further text bytes observed before completion,
waited 5005 ms for settlement notifications (none), then ended its owned job in
15 ms. Text bytes are not tokens and are not a monetary reserve.

The receipt's historical `stopToResultMs: 5007` includes the settlement drain;
this is not a claim of 5007 ms interrupt acknowledgement latency. The helper now
records acknowledgement and drain-end times separately. No additional model turn
was run after confirming this failure. Three stops were planned for Codex; that
series stopped at its first failed measurement. Claude interruption measurements
were not started. No maximum token overshoot or safety-factor reserve is invented.
`Budget::can_admit` has no newly measured bound; public role admission remains
closed and non-configurable. The captured unmetered interruption is replayed
through the production parser/ledger to prove its reservation stays unsettled,
measured spend stays unknown, and continuation cannot return/reuse the allowance.

## Installed identity and native controls

| Vendor | Version | Resolved executable SHA-256 |
| --- | --- | --- |
| Claude Code | 2.1.296 | `76de74f7694a5bdc03378a5f837c9e132827bf004c98c276bc4e9094ff06769a` |
| Codex | 0.162.1 | `dd13bdb162a184fa04039488b234a8258ef9b76f84e926c505d5742b19249a6a` |

The fresh preflight confirmed subscription billing. Codex paid fallback was false;
the completed Claude native turn reported overage inactive and paid fallback false.
Only resolved executable files were hashed under R112. No CLI credential or user
configuration file was opened directly. The official `config/read` reply retains
MCP names only through `IgnoredAny`; values are neither copied nor recorded.

Codex's [version-matched configuration schema](https://raw.githubusercontent.com/openai/codex/rust-v0.162.1/codex-rs/core/config.schema.json)
was inspected. Its native tools table does not establish an `apply_patch` removal
control. The `apply_patch_freeform` feature exists, but was not treated as a verified
removal guarantee or replaced with a guessed key. The installed patch tool remains
offered; its real writes are approval-gated and declined. The
[app-server protocol](https://developers.openai.com/codex/app-server) documents the
thread sandbox/approval controls; installed behavior is the conformance evidence.

Both vendors report zero inherited tools and zero active inherited MCP servers.
Codex still reports four explicitly disabled registry entries, each with zero tools,
resources and templates; hooks count is zero and plugin inventories are empty.
Claude reports empty tools/MCP/skills and zero user plugins. Its three expected
embedded modules remain metadata. Claude startup does not report a hook inventory;
that observed count remains unknown despite the fixed hook-disable settings.
No complete private admission receipt is claimed from these partial observations.

## Cross-vendor review disposition

| Finding | Verified result and action |
| --- | --- |
| 1. Red check bypasses dispatch | Accepted and fixed. The in-memory verify closure is removed. The hidden fixture invokes `Runner::run_proof` and `proof::execute`, starts the first-party checker in an owned subprocess with recorded isolated environment, and verifies real red exit 1 / green exit 0. Spawn failure is persisted as `NotRun` and cannot verify a tree. Private logs are bound to the receipt. |
| 2. Scripted build/canary | Accepted scope limitation, already labeled scripted; made explicit again. Case 4 copies the prebuilt test launcher and injects `CanaryVerdict::Pass`. It proves the fixture lifecycle, not candidate compilation or the mandatory miniature mission. No real build/canary ran after the spending stop. |
| 3. Resume consumption | Accepted missing positive proof. Case 6 currently proves denial twice and preservation of the fixture grant. A successful Resume consuming the grant exactly once requires valid receipt admission; it remains pending after G2, with no gate bypass. |
| 4. Dummy stable hash | Accepted limitation. Case 7 still concerns disposable fixtures only. Separate read-only before/after manifests now hash the actual owner's 291 tracked files, HEAD/index/packed-refs, and both real install files. All compared bytes match across the live probes. Untracked files and build outputs are excluded. A final post-validation audit is recorded separately. |
| 5. Marker and receipt consumer | Marker readback fixed: both vendors compare exact bytes and before/after SHA-256 after shutdown. The no-receipt-consumer finding is accepted; fresh version/hash-bound admission remains unimplemented and closed after G2. |
| 6. Known literal unknown collision | Rejected as a defect in this existing format; documented and covered explicitly. The scalar string `unknown` is reserved for missing evidence. Known string metrics are validated git object IDs, which cannot equal that marker; token/time metrics are numbers. Arbitrary free-text known metrics would require a format change. |
| 7. Dirty owner files contradict unchanged | Rejected after inspecting the preserved pre-run manifest: `.gitignore` and `docs/roadmap.md` were already dirty before that run. `unchanged` compares against that dirty baseline, not git HEAD. The original audit now includes baseline status, evidence path and that meaning. No owner file was restored or modified. |

The red/green dispatch still uses first-party fixture candidate content and scripted
review records. Its real subprocess does not turn the fixture into live acceptance.
Cases 4-6 use fixture canary evidence and a prebuilt launcher. Successful real
Resume, real candidate compilation and a writer/proof/other-vendor miniature
canary must still be exercised in the eventual real run.

## Evidence and validation

Authoritative private evidence root:
`C:/Users/Erick/AppData/Local/Faber/t13-exposure-20261014/`.

- `red-native-completion.log`: pre-fix accepted completed native file change.
- `red-proof-dispatch.log`: missing production dispatch coordinator before implementation.
- `g1-preflight/qualification-preflight.json`: fresh installed versions/hashes and billing metadata; zero model turns.
- `g1-codex-role-tools/live-denial.json`: real approval/decline, exact before/after inside-workspace bytes, marker hashes.
- `g1-codex-native/live-denial.json`: six native checks, loadout metadata, unchanged outside/marker hashes, settled counters.
- `g1-claude-native/live-denial.json`: empty tools, native checks, exact external bytes/marker hashes, settled counters.
- `g2-codex-stop/live-denial.json`: first unmetered interruption, no delayed-settlement claim.
- `g2-codex-stop-drained/live-denial.json`: repeated unmetered interruption with five-second drain; stop rule 2.
- `g1-owner-before.json`, `g1-owner-after-probes.json`: actual tracked-owner and install hash inventories, equal bytes.
- validation-native-authority/proofs.json and proof-01.log through proof-07.log: all required commands exit 0. g1-owner-after-validation.json: actual owner/install hashes still match. summary-native-authority.json: reduced measured summary; delivery/CI are recorded separately.

## Cost and RAM

This continuation requested 10 tiny model turns: seven Codex native checks, one
Claude native check and two Codex interrupted metering turns. The eight completed
native turns have measured API-equivalent subtotal **$0.00721924**. The interrupted
turns are unpriced; this subtotal is not the total cost. The previous continuation
had known subtotal $0.00577606 and two additional unsettled turns. Combined known
subtotal is **$0.01299530**, with four unsettled turns; total spend remains unknown.
These are API-equivalent token accounts on subscriptions, not a cash API invoice.

The real acceptance mission never ran; its cost and RAM are unknown. No peak live
CLI RAM was sampled for these tiny probes. Previous scripted Projects peak was
221.7 MB and warm fixture app/tree was 1.1/140.9 MB; these are not real-mission RAM.
Fresh suite totals and scripted RAM are recorded only after the required run ends.

## Gate table

| Gate | Result | Evidence / remaining work |
| --- | --- | --- |
| G1 Authority | Partial; former breach fixed | All-role/start/resume controls and native denial/byte proofs pass; matching admission receipt and writer integration pending. |
| G2 Spending | **Failed for current metering strategy** | Two genuinely interrupted Codex turns remain unmetered, including delayed drain. No measured reserve. |
| G3 Scheduling | Not passed | No admitted live mission graph. |
| G4 Acceptance | Not passed | Real fixture subprocess proofs; no real accepted candidate/review. |
| G5 Recovery | Not passed | Fixture rollback/Resume denial only; successful real Resume pending. |
| G6 Self-update | Not passed | Prebuilt/injected fixture only; miniature mission and real build pending. |
| G7 Screen/resources | Not passed | Scripted screen/RAM evidence only; complete live lifecycle absent. |
| G8 Real use | Not passed | Fresh Seguro target mission not dispatched. |
| G9 Kits | Partial | Zero inherited tools; ungranted native write blocked; full receipt/canary and hook attestation pending. |
| G10 Software bar | Not passed | Full real acceptance absent. |
| AS-1 Motion | Passed, scripted | Hidden fixture frame checks only. |
| AS-2 Composer | Passed, scripted | Hidden fixture growth/cap/scroll/shrink checks. |
| AS-3 Breadcrumbs | Passed, scripted | Hidden five-locale checks; no new native-speaker review. |
| AS-4 Gap | Passed, scripted | Hidden 64 px fixture checks. |
| AS-5 Window audit | Partial | Owned hidden jobs used; complete mission/update audit absent. |
| AS-6 Disk | Not passed | No real promotion/cleanup; evidence retained. |
| AS-7 Specific plan | Not passed | No admitted real helper plan. |
| AS-8 Dual vendors | Not passed | Both native probes ran; no real cross-vendor review/miniature canary. |
| AS-9 Only needed questions | Not passed | No real lead question log. |

## Remaining sequence

Repair/enforce interrupted spending metering or a complete invocation maximum,
then measure several safe stops per vendor and consume the maximum with a safety
factor in `Budget::can_admit`. Only then implement private fresh version/hash-bound
receipt admission, integrate the Windows owned-process proof into writer dispatch,
run the miniature canary, and execute the runbook's $2 Seguro helper mission in the
fresh target clone with the other vendor reviewing. Build/install only in the
approved sandbox, exercise rollback and successful exactly-once Resume, measure
cost/RAM and write the real acceptance report. No real clone, install or promotion
was attempted after the spending failure. Existing admission stays fail closed.

## Final local verification

All seven required commands exit 0 before committing the two independently staged
steps. No implementation changes follow this verification. The full suite reports
**1377 passed, 0 failed, 0 not run, 0 skipped**, in 1202.186 seconds.
The eight pure qualification-helper tests also pass. Scripted Projects peak is
**224.8 MB**, with warm fake guard / whole tree **1.1 / 144.5 MB**. These are
fixture measurements; real mission and live-probe RAM remain unknown. The final
read-only audit still matches all 291 tracked owner files, HEAD/index/packed-refs,
and both stable install files. The checked-in API-equivalent rate date is 2026-10-12.

The accompanying [reduced native-authority evidence](2026-10-14-self-development-native-authority.json)
records command exits, timing, settled token counters and unknown interruption cost.
The PR remains a draft. Final pushed-head and CI status are recorded separately in
private delivery-native-authority.json; CI success is never inferred from local tests.

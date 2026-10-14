# Task 13 acceptance: stopped, not accepted

Initial measurements: 2026-10-13, America/Fortaleza (UTC−03:00). Baseline `dc46d45`;
branch `selfdev-t13`. Repeated validation may cross into 2026-10-14; this file names the initial
measurement date. **Task 13, G8 and R96 remain incomplete.** October 22 is an internal milestone,
not permission to reduce the acceptance bar (R105).

## Phase disposition

| Phase | Result | Evidence |
| --- | --- | --- |
| A | Partial: seven hidden safety checks pass; the complete scripted mission and successful Resume are not proven. | [Scripted scope and regression](2026-10-13-self-development-scripted.md), `engine/tests/project_report.rs`, `testkit/src/screen/{projects,local_update}.rs`. Commit `3565e2c`. |
| B | Stopped. No complete containment/loadout qualification, fresh version/hash-bound pass receipt, or verified invocation cost bound. Admission stays closed. | [Qualification disposition](2026-10-13-self-development-qualification.md), [informational diagnostic](2026-10-13-self-development-qualification.json), `testkit/examples/project-qualification.rs`. Commit `82394f0`. |
| C | Not executed after the stop. No real mission, local landing, build, miniature canary, promotion or successful Resume. | This report and the private stop/audit records. |

The known report defect was reproduced first (`Known("unknown")` after deserialization), then fixed
with an explicit missing-evidence sentinel reader. Existing scalar JSON remains compatible with the
previous build. Other known strings and numeric metrics retain their scalar encoding.

Case 6 is not replaced by a positive claim: the public `Runner::resume` refuses unqualified roles
after native rollback, twice, without changing the pre-rollback $2 fixture grant. Existing pure seam
tests prove authorized reconciliation and duplicate-charge rejection, not a live successful Resume.
Fixture canary and other-vendor review records authorize only the scripted lifecycle assertions.
The red/green fixture check is an in-process condition in the native test helper, not a `cargo test`
through an admitted worker. Writer/reviewer queue records are fixture values. These checks exercise
the proof/landing/report contracts, not a complete fake-CLI mission through production admission.

## Stop point

The hidden no-model preflight ended with `protected resolved path` while attempting an installed
binary fingerprint. It refused the file open. The original failure diagnostic did not preserve the
vendor identity or preceding status observations; those remain unknown. The diagnostic was captured
in a second no-model attempt before the stop. No installed CLI preflight was replayed after that point.
The helper now retains a stopped diagnostic receipt on a binary fingerprint failure and filters
protected PATH roots; four pure helper tests pass. These improvements were tested without live CLIs.

Runbook stop rule 2 also independently blocks any model probe: no proven invocation maximum or measured
stop-cost reserve exists. A 120-second deadline and an estimated CLI budget do not establish a dollar
bound. No native-tool escape prompt, planted-canary read, real credential read, or model turn was sent.
Neither subscription billing nor disabled paid fallback is claimed from missing account observations.

There is no passing private qualification receipt. `policy::admit` is still non-configurable and refuses
every role. Matching/fresh receipt admission and requalification after CLI changes remain unimplemented.
The live writer launcher and miniature canary remain blocked as documented in the Phase B report.

## Planned real run, deliberately unspent

The intended goal remains candidate 1: pure Rust `format_disk_bytes` in
`engine/src/project/disk.rs`, with `engine/tests/project_disk.rs`. It was not implemented outside the
mission to manufacture G8 evidence. Intended mode: Seguro, one vendor writes and the other reviews.

The planned grant was $2.00 API-equivalent: lead $0.30, writer $0.60, reviewer $0.40,
canary/repair buffer $0.70; maximum turn 120 seconds; pre-dispatch stop reserve $0.15;
both account windows stay within the 1/7 daily pace. No real mission grant or dispatch was created.

`E:/Documentos/ProjetosIA/faber-t13-target` and
`E:/Documentos/ProjetosIA/faber/target/t13-target` were not created. No target clone or sandbox install
needed deletion. No mission push or PR occurred. The outer development draft PR is explicitly authorized
by the owner and is separate from mission authority.

## Measurements and exact evidence

| Measurement | Real mission | Scripted evidence (Phase A) |
| --- | --- | --- |
| Uncached input U | Unknown: no mission ledger | All unmeasured role fields remain `Unknown`. |
| Cache read R | Unknown: no mission ledger | Same. |
| Cache write W | Unknown: no mission ledger | Same. |
| Output O | Unknown: no mission ledger | Same. |
| API-equivalent mission cost | Not measured; no real mission | Fixture amounts are not subscription spend. |
| Model turns requested by qualification | 0 | Helper sends only version/status requests; no model prompt. |
| Real-run peak RAM | Unknown | Projects board peak 232.0 MB; idle full/reduced motion 112/113 MB; warm fake-CLI app tree 132.9 MB. |
| Red / landing / report / unchanged scenario time | Not measured | 4.0 / 4.0 / 4.0 / 4.0 seconds. |
| Promotion scenario time | Not measured | 334.6 seconds, including full 300-second probation. |
| Early-crash scenario time | Not measured | 13.8 seconds for the whole scenario; restore asserted within a 30-second wait. |
| Resume safety scenario time | Not measured | 13.9 seconds for the whole rollback/refusal scenario. |
| Time and token savings | **Unknown** | No paired plain-CLI baseline. |

One scripted persisted-report run in the Phase A full log records commit
`e4536d833cb1de2d53622a69cfd34d39e225d1da` and tree
`393c9f0cefb01f5043e2fe2d2b9b773eeaa9a162`. The red receipt records exit 1; the landed exact-tree
receipt records a genuine passing check. These are temporary fixture OIDs, not an accepted Faber revision.
There are no real `review.json`, accepted commit/forward receipt, promoted manifest or rollback receipt
to present as G8 evidence. The native startup handshake honestly returns `CanaryVerdict::NotRun`.

Authoritative diagnostic and audit records are retained at
`%LOCALAPPDATA%/Faber/t13-acceptance-2026-10-13/`. Repository copies are informational only (R111).
The private records include the failed preflight log and its SHA-256, the stopped diagnostic, the complete
owner/install before manifest, the after audit, and full proof logs. No credential or session-token file
was captured. The process count and CLI version/hash observations lost by the initial diagnostic remain unknown.

The original checkout audit covers 270 entries: tracked source bytes plus HEAD/index/packed-refs.
All compared bytes and the tracked path set remain equal; the original dirty `.gitignore` and
`docs/roadmap.md` are preserved. Untracked files and build outputs are excluded, so whole-folder byte
identity is not claimed. The original index hash is
`6A4770FCDC4412CB34CC1967283B7AAEC3ACFA83A48701505E8CF87FE549E794`.

All two stable install files match their before hashes:

- `faber.exe`: `076ED666FBDDB70A3052D0218C1AA0D730A3444AF740692CD4F3DA275FC0F230`.
- `uninstall.exe`: `1AB5D119C34E5C58E980D17FB1EA9C37FCB1FF755913C9341BB91C6D549AD34B`.

No running owner `faber.exe` was stopped. No stable install write or original `target/debug` access
was performed. Builds use only the prescribed `target/t13` and its hidden-test child target.

## Gate table

Pass below means the named acceptance condition has observed evidence; it does not imply other gates pass.

| Gate | Result | Evidence or reason |
| --- | --- | --- |
| G1 Authority | Not passed | Owner/stable audit and fake broker denies pass; installed role restrictions, revocation during live effects and complete checkout identity are not proven. |
| G2 Spending | Not passed | No real settled U/R/W/O ledger; no verified invocation bound, stop reserve, billing/fallback or calibrated daily headroom. |
| G3 Scheduling | Not passed | Pure scheduling/lease tests pass; no durable real run graph showing ready dispatch and all four brakes. |
| G4 Acceptance | Not passed | Scripted exact-tree proof/review/CAS landing passes; no admitted other-vendor review or real accepted candidate. |
| G5 Recovery | Not passed | Native fixture rollback, unchanged grant and pure duplicate-charge tests pass; successful live Resume and all live intent boundaries are unproven. |
| G6 Self-update | Not passed | Fixture promotion/early-crash recovery pass; production miniature mission returns `NotRun`, so production promotion stays blocked. |
| G7 Screen/resources | Not passed | 232 MB Projects fixture and hidden audit pass; five-locale/width live mission coverage and native-speaker review are incomplete. |
| G8 Real use | Not passed | Real Seguro helper mission not dispatched after qualification stop. |
| G9 Kits | Not passed | No complete current session tools/MCP/plugins/hooks attestation, live assembly p99 or escape qualification. |
| G10 Software bar | Not passed | No complete real P/J acceptance receipts; calendar bar remains intact. |
| AS-1 Motion | Passed | Rendered `projects AS-1` observes intermediate frames; reduced/none disable travel. Full hidden-build run passes. |
| AS-2 Composer | Passed | Rendered `projects AS-2` observes wrap growth, 40vh cap, scroll and clear shrink. |
| AS-3 Breadcrumbs | Passed | Rendered `projects AS-3` checks actual selected locale and `›` in all five locales. This is not native-speaker approval of all copy. |
| AS-4 Gap | Passed | Both rendered AS-4 checks measure exactly 64 px, including single/multiple composer heights. |
| AS-5 Window audit | Not passed | Scripted native mission/update audits report zero visible windows; no real dual-vendor mission/canary/promotion audit exists. |
| AS-6 Disk | Not passed | Scripted owned cleanup/low-space tests pass; no post-promotion real-run cleanup receipt. Unused target clone/build were never created. |
| AS-7 Specific plan | Not passed | Pure validator tests pass; no admitted live lead plan for the helper goal. |
| AS-8 Dual vendors | Not passed | Fresh fixture cross-vendor review passes; no real other-vendor acceptance or promotion review. |
| AS-9 Only needed questions | Not passed | No real lead question log; an absent mission cannot establish question classification or restraint. |

## Validation and disposition

Phase A: all seven required proof commands exit 0; `cargo t --all` reports
**1,370 passed, 0 failed, 0 not run, 0 skipped**. Full logs and per-command exit/time records are retained
in private data, with informational local copies in `target/t13-evidence/`.

Phase B: the same seven commands exit 0, again **1,370 passed, 0 failed, 0 not run, 0 skipped**.
The manual preflight's four pure example tests also pass. These passing tests do not change the
failed qualification verdict. Validation continued into 2026-10-14 local time.

Phase C report validation: all seven commands exit 0; **1,370 passed, 0 failed, 0 not run, 0 skipped**.
See the informational [per-command proof summary](2026-10-13-self-development-proofs.json) and
[owner/install after audit](2026-10-13-self-development-owner-audit.json). The real mission still did not run.

Further phase proof results and final branch/CI identity are recorded with the delivered commits and draft
PR. The draft must remain unmerged. No roadmap or spec completion box is marked from fixture evidence.
Remaining work is the full installed-version qualification, enforceable live spending, owned writer dispatch,
the cross-vendor miniature canary, and then the granted real helper mission with promotion, rollback and Resume.

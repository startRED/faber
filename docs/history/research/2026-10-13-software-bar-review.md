# T5B cross-vendor review

Reviewed branch `selfdev-t5b`, implementation `b163d850e016f531968b576abb4893e5346df29f`.
Original findings: 3 critical, 14 high, 3 medium. Post-fix backend coverage: 79 done, 30 partial, 8 missing.
The branch does not prove complete T5B delivery or G10. Counts distinguish implemented guards from live execution.
Sources: repository AGENTS.md and roadmap; rework-fit.md G10 MVP and complete software bar rows;
generated-pipelines design status note and sections 3, 5–9, 13, 15.1; rework design superseding sections.
The benchmark and real G8/G10 gate belong to T13; runner, hidden ghost orchestration and slot qualification also require T5/T9/T11.
No source-only test can establish those live gates. This review does not waive E10 or equate fixtures with live evidence.

## Findings at b163d85

| Severity | Location | Concrete failure |
|---|---|---|
| Critical | engine/src/project/wire.rs:421; engine/src/project.rs:591 | A lead submits a sheet with an empty bar. Envelope validation discards sheet/bar when building the temporary Plan. No production caller invokes the compiler/mandatory guard. |
| Critical | engine/src/project/bar.rs:1053 | Keep `rust.fmt` ID but change mandatory to false, proof to `always.pass`, severity to minor or pass criterion to anything. Guard accepts each. |
| Critical | engine/src/project/proof.rs:118 | Any string of length 64 is accepted as an evidence hash. Empty/duplicate outcomes and missing required checks have no acceptance validator; a valid not-run receipt can be mistaken for completion. |
| High | engine/src/project/proof.rs:20 | Inputs `["a;b"]` and `["a", "b"]` hash identically. Dependencies/environment have the same collision. Inputs contain paths in the claimed test, not captured file content. |
| High | engine/src/project/proof.rs:78 | Invalidation deletes only by_task. The stale by_digest pass survives changing the task or a dependent. Malformed passes are also cached. |
| High | engine/src/project/bar.rs:1328 | A check's nonexistent owner is accepted if another task mentions its ID. Unknown checks, cycles, missing evidence and missing dependencies are not checked here. |
| High | engine/src/project/bar.rs:1213 | Critic replaces an existing mandatory requirement by matching attribute, removing its ID and mandatory flag. |
| High | engine/src/project/bar.rs:1066 | Repository ground can read `../outside.md`, an absolute path or a link outside the repo. No heading is required. URL validation accepts attacker/loopback hosts, HTTP, redirects and unfetched text. |
| High | engine/src/project/bar.rs:35 | All checks are assigned to the first change/screen only. More pieces receive no bar. Library itself exceeds judged caps on a single change piece; nonmandatory ghost/draft/coherence checks can be deleted. Conditional kinds never switch groups. |
| High | engine/src/project/bar.rs:1246 | Moves are enum lists. Ask/guard and unconditional program measurement are missing; drafts/ghosts have no execution record, and failed spend paths always return to drafts. |
| High | engine/src/project/proof.rs:232; :264 | Registry check never verifies existence/publisher/age/downloads. OSV check only searches a custom fixture list; an empty list implies no findings even with no coverage or offline query. No lockfile inventory or public query exists. |
| High | engine/src/project/proof.rs:156 | Scanner echoes the entire line containing a PAT into a violation. Byte slicing after AKIA can panic on Unicode. It misses multiple common secret forms. |
| Medium | engine/src/project/proof.rs:185 | Valid SPDX expressions such as `MIT OR Apache-2.0` are rejected. No Cargo/package metadata extraction or expression handling exists. |
| High | engine/src/project/store.rs:487 | Compile receipt overwrites the same revision without validation. Reads do not validate candidate, phase completeness, check IDs or artifacts. Sheet has no durable version field and rejects unknown future fields. |
| Medium | engine/src/project/bar.rs:1478 | Prefix matching treats `ui/App.tsx.bak` as owned by `ui/App.tsx`, and read-only task paths claim repair ahead of the writer. It does not inspect held leases. |
| High | engine/tests/project_bar.rs:556; :639; :887 | Draft/ghost tests assert static arrays; the alleged orphan-evidence test never creates orphan evidence; cached not-run is fetched and merely asserted not to be a pass. They do not prove the claimed live rules. |

## Baseline evidence

The original 31 project_bar tests and 6 wire-v2 tests passed. Ten independent regression tests added before edits all failed
against b163d85 (mandatory weakening, critic overwrite, hash collision, malformed cache pass, stale digest invalidation,
missing owner, cycle, sheet with empty bar, escaped/no-heading ground, secret echo). These are executable counterexamples.
Wire v2 strict nested unknown-field rejection and old durable Plan extra-field roundtrip already work. Rejecting old v1
worker envelopes is the intentional T5K revision; it must not be confused with breaking old durable records.

## Coverage at the reviewed commit

Each row enumerates required items, not claims from a commit message. `partial` means definitions/helpers or some existing
proofs exist; `done` requires executable enforcement and defect-sensitive tests; `missing` means the claimed behavior has no implementation.

| Required item | Status | Evidence / gap |
|---|---|---|
| Sheet: change/screen/doc contexts, seven saved answers | partial | DTO/save helpers, no lead integration; unknown risk accepted |
| Sheet: public/device facts, provenance, ≤3 result-changing defaults | partial | Public defaults only; no device seed; structural question/provenance checks |
| Bar: every B1–B10 rule retained per piece | partial | 67 labels; several subrules omitted and only first piece covered |
| E10: lead cannot remove/weaken required checks; not-run blocks acceptance | missing | ID-only guard has no mission/acceptance caller |
| Grounds: scoped repo heading; scoped official fetched URL | partial | Heading helper unsafe; URL syntax only |
| Finder: software-only miss, isolated drafters, merge, critic | partial | Merge/critic helpers, no isolated sessions or finder |
| Caps: ≤12 judged, ≤6 blocking, ≤30% judgment-only per piece | partial | Filter exists but library violates caps and is not filtered |
| Moves: ask, reuse, diverge, measure, in-place, guard, judge, gate by trigger | partial | Some enum lists, missing guards/ask/measure |
| Moves: 2–3 judged drafts at real widths before code | missing | Constant width array is not execution |
| Moves: four hidden-app other-vendor ghosts; stalled/spend repair | missing | Constant persona array; repair target always diverge |
| Fast path: one existing attribute, reversible/no risks, mandatory guard, full fallback | partial | Four-field boolean helper only |
| Compiler: orphan check/evidence/owner, scoped nodes | partial | Incomplete graph helper; no evidence graph/entry point |
| Compiler: back-chain/prune, cheap proof first, ≤200 ms/200 nodes | partial | Pure functions/test; no production compile |
| Compiler: held failing-path lease owns repair | missing | Task path prefix lookup only |
| Rerun: brief/input/loadout/model/base/toolchain/deps/env digest | partial | Ambiguous concatenation; caller-provided paths |
| Rerun: identical reuse, reader/descendant invalidation, no unknown pass | partial | In-memory maps disagree; no durable rehydration |
| Receipts: all phase times/cost tags; every check result/reason; exact candidate | partial | DTOs, weak validators and unvalidated journal reads |
| B1: risk/callers/red-green/registry/quick-secret-before-review/full/UI/receipt | partial | Labels plus existing general proofs; no proof dependencies or test-first chronology |
| B2: spend first/drafts/ghosts/staging/keys/failure routing | partial | Staging/keys existing T4 safety; remaining labels |
| B3: fmt/both clippy/lock lint; input panic/blocking/unsafe/key audits | partial | Existing proof commands; audit labels only |
| B4: shell/npm/hidden tree/atomic/version/old readers/crash/memory/process/credentials | partial | Existing T1/T2/testkit proofs, not bound to check receipts |
| B5: flagged inventory/CSP/remote IPC/scoped inputs/signature/channel/webview | partial | Existing shell checks; inventory and receipt linkage missing |
| B6: lint/types/build/reactivity/cleanup/window40/five locales/tokens/duplicates/all widths/themes | partial | Existing UI suite; all-screen width/theme coherence absent |
| B7: rendered contrast/targets/keyboard/focus/motion/text | partial | Existing keyboard/motion tests; full rendered WCAG evidence absent |
| B8: lock inventory/OSV/name-version-only queries/offline unknown | missing | Synthetic exact-version fixture matcher only |
| B8: metadata/SPDX compatibility/registry/publisher/age/downloads/typos | partial | Whitelist and edit distance only |
| B8: native secret scanner/no telemetry/logged authorized destinations/malicious fixtures | partial | Scanner unsafe; no query destination receipts |
| B9: feature red; bug reproduction/callers; refactor golden/API/schema/bench; upgrade changelog | partial | Labels, missing refactor API/schema/benchmark checks |
| B10: root regression/empty-huge-Unicode-offline-timeout-two-windows/compatibility/2s/docs/English commit | partial | Labels; two-window probe omitted; existing separate tests |
| Software benchmark, ≥95% parity, both-order blind/plain baselines, honest cost | missing | T13 responsibility, no T5B result |
| G10 on real G8, all required ran/pass, applicability evidence, G1–G9/canary/rollback | missing | T5/T9/T11/T13 integration absent; never claimed passed here |

## Fix and mutation evidence

The original completion claim is rejected. Backend corrections and actual mutation results are recorded below;
live qualification gaps remain explicit in the post-fix coverage table.

## Additional counterexamples found during fixing

| Severity | Location at reviewed commit | Concrete scenario / disposition |
|---|---|---|
| High | engine/src/project/wire.rs:447 | Every generated dotted proof ID is rejected by the proof-request parser. A writer cannot request `proof.cargo-fmt`. Fixed by the same bounded dotted-ID validator used for bar proofs. |
| High | engine/src/project/wire.rs:329; engine/src/project/lease.rs:155 | A `.codex/auth.json` read inside a worktree passes the wire and broker text guards. A temporary synthetic credential-directory document demonstrated the omission without touching real credentials. Fixed through the existing kit path policy at both boundaries. |
| Medium | engine/src/project/bar.rs:1082 | A `# Fake heading` inside a Markdown fence qualifies a supposed repository ground. Fixed by distinguishing real headings, fences and indented code. |
| High | engine/src/project/proof.rs:185 | Noncommercial purpose automatically allows GPL/AGPL/LGPL dependencies without proving compatibility with the repository license. A new negative test failed on this shortcut; unsupported terms now require review in either purpose mode. |

Two review fixes also caught their own regressions test-first: stricter journal loading initially rejected older software
metadata, and a SHA-256-only candidate constraint rejected Git's SHA-1 composed trees. Both were corrected before the
second commit. Async source receipt writes initially blocked Tokio; an injected disk waited for a timer on the same
runtime and failed before the writes moved to the blocking pool. A final lifecycle regression also caught an immutable
compile receipt with no future check IDs: compilation now reserves task-qualified IDs before dispatch, so proofs can
arrive afterwards without rewriting the receipt. A malformed Cargo header could silently disappear beside a valid npm
inventory; unsupported headers or a missing Cargo package section now fail closed. These are review-generated issues,
not attributed to Gemini. A scoped npm package can be private even on the public npm host; the new native path now verifies unauthenticated registry metadata before any OSV disclosure. A failing journal stops both the baseline counterexample and its mutation before HTTP transport, so this privacy test never contacts a server.

## Post-fix coverage, by requirement

`done` below refers to implemented, executable backend enforcement with a defect-sensitive test. It does not mean that
a real G8 mission ran, that a model actually judged a fixture, or that G10 passed. `partial` identifies integration or
live evidence still required by T5/T9/T11/T13. No required unknown/not-run check is relabeled pass. Known change-kind
exclusions retain the check, its reason and evidence; unknown kinds retain every obligation.
Saved sheets now have a schema/mission/extension-bearing companion record; `sheet.json` retains its old display shape.
Newer companion schemas are refused, extension fields survive replacement, and historical raw sheets remain readable.

| Required item | Status | Code / proof |
|---|---|---|
| Change, screen and doc piece contexts | done | Sheet validation and per-piece library coverage regressions |
| Seven risk fields saved | done | Sheet/journal roundtrip and field validation |
| Public-user seed | done | lead.rs:221, seeded fact with source |
| No-other-data seed | done | lead.rs:221, seeded fact with source |
| Windows/macOS/Linux device seed | done | Compilation/journal fact regression |
| At most three questions and valid defaults | done | Original negative question tests and sheet mutation |
| Only materially result-changing questions | partial | Nonempty changes validated; semantic relevance needs lead execution |
| Assumption provenance preserved | done | Existing provenance rejection and sheet roundtrip |
| Every original B1–B10 obligation retained | done | Library @2, per-piece canonical checks and static inventory tests |
| Lead cannot delete mandatory checks | done | Removal tests and actual source mutation |
| Lead cannot weaken a check under its existing ID | done | Seven concrete payload changes, full semantic comparison |
| Conditional checks cannot be silently removed | done | Canonical preservation and explicit applicability outcomes |
| Ground path scoped inside repository | done | Existing broker link-safe IO, traversal regression |
| Real cited Markdown heading | done | Missing/fenced/indented/non-heading regressions |
| Credential homes excluded from grounding | done | Synthetic credential-directory regression |
| Official URL host/HTTPS/userinfo/port checks | done | Scoped URL tests and source-policy mutation |
| Official URL fetched and destination logged | done | Native fetch with intent/outcome records and bounded shared reader |
| Quote present in fetched source | done | Grounded compile validates fetched bytes and quote |
| Source semantically supports the added check | partial | Requires admitted critic; substring presence is not semantic support |
| Software-only finder scope | partial | Software-specific merge entry point; live miss dispatch is T5 |
| Two distinct read-only draft vendors/sessions | done | merge_software_miss identity/role guards and negative tests |
| Actual isolated draft model calls | missing | No admitted runner/session execution in this branch |
| Merge scoped by piece and normalized attribute | done | Cross-piece merge regression |
| Critic from another vendor in fresh session | done | Identity/role/session validation |
| Actual critic model call | missing | Not inferred from recorded additions |
| Critic additions capped at three | done | Original negative cap tests |
| At most twelve judged checks per piece | done | Library cap test plus compiler enforcement |
| At most six blocking judgments per piece | done | Original cap test plus compiler enforcement |
| At most thirty percent judgment-only checks | done | Original negative ratio tests |
| Ask move by question trigger | done | Move regression |
| Reuse move before new/reworked screen | done | Screen trigger; repository/product-map execution is T5/T9 |
| Actual repository/product-map reuse search | partial | No live mission dispatch |
| Diverge move on screen | done | Existing trigger regression |
| Two or three complete width draft records before code | done | screen.rs:20 negative validation and mutation |
| Real judged drafts at 720/900/1100 | partial | Validators exist; actual draft execution is not recorded here |
| Measure on program work | done | Unconditional measure before judge regression |
| Guard on all work | done | Move regression |
| In-place move on screen | done | Screen trigger |
| Four distinct ghost persona records | done | screen.rs:63 negative validation and mutation |
| Ghost must use hidden app | done | Hidden-run record guard; actual window provenance is runner-owned |
| Distracted ghost interrupted at narrowest width | done | Record guard and negative test |
| Actual other-vendor ghost sessions on changed flow | missing | Requires T5/T9 execution, not static persona arrays |
| Screen failure returns to drafts | done | failed_check_target regression |
| Failed spend path returns to held writer | done | Held/released lease and path-boundary regressions |
| Other-vendor judge move | done | Move and captured-session validation |
| Promotion/self-update safe gate move | done | Unconditional gate; actual promotion remains T5/T11 |
| One existing attribute/fact fast path | done | Exact known-reference check; multiple-attribute string rejected |
| Reversible/no newly enabled risk fast path | done | Actual before/after risk comparison, not caller flag alone |
| Fast-path cache reuse and full-compile fallback in live runner | partial | T5 queue integration absent |
| Reject orphan check owner | done | Owner/proof binding regression and mutation |
| Reject orphan evidence producer | done | Named proof declaration and scoped-path validation |
| Reject generic nodes without check/path | done | Original and new negative tests |
| Reject unknown checks, dependencies and cycles | done | Existing graph validator reused before dispatch |
| Back-chain and prune unused work | done | compile_software and dropped-node journal test |
| Cheap proof before judgment | done | Topological cost order test |
| Failing path repair uses held lease | done | repair_lease and failed_check_target |
| Compiler at 200 nodes within 200 ms | done | Existing executable measurement retained |
| Brief/input/loadout/model/base digest | done | Unambiguous serialization and mutation |
| Toolchain/dependency/environment digest | done | Field-change tests; runtime environment additionally captured |
| Actual source content captured, not filenames | done | Link-safe capture_inputs and content-change test |
| Stale caller input snapshot rejected | done | Current input hashes verified again at reuse |
| Identical durable result reused across restart | done | Private cache restart test |
| Changed reader/descendant invalidation only | done | In-memory graph invalidation test; task-qualified durable keys |
| Fail/unknown/not-run never reused as pass | done | Outcome validation and non-pass cache tombstone test |
| Live queue consumes cache correctly | partial | T5 integration absent |
| Five compile phase timestamps/cost tags | done | Complete/order/tag/overlap regression |
| Every check outcome/reason retained | done | Missing/duplicate/outcome validation; known exclusions explicit |
| No inferred receipt pass | done | Missing/bad evidence tests and mutation |
| Same exact accepted candidate | done | Candidate and plan digest binding; Git SHA-1/SHA-256 accepted |
| Immutable private acceptance authority | done | Public-copy tampering and replacement regression |
| Evidence bytes match recorded SHA-256 | done | Real file-tamper regression and mutation |
| Older durable records readable | done | Old software metadata/compile receipt and extra-field roundtrips |
| Active wire unknown fields refused | done | Existing strict v2 nested-field tests retained |
| B1 risk answers/caller map | partial | Risk persistence works; actual caller scout evidence is T5 |
| B1 base-red/change-green chronology | partial | Retained required rule; live proof runner must capture chronology |
| B1 registry existence and exact version | done | Native metadata query/parser and negative version test |
| B1 publisher, age and downloads | done | Missing-metadata rejection; native public endpoints |
| B1 typo check before use | done | Bounded original heuristic plus metadata verification entry point |
| B1 quick tests/secrets before exact-tree review | partial | Required checks/order; admitted exact-tree reviewer absent |
| B1 real screen/keyboard/theme evidence | partial | Existing hidden screen suite runs; no new mission linkage |
| B1 full suite and save receipt | done | Required commands run; durable compilation/check records |
| B2 spend paths tested before code | partial | Required rule retained; live T3/T5 spend proof linkage absent |
| B2 staging/next-only and no signing credentials | partial | Existing T4/T11 native guards; no new G8 run |
| B3 fmt and both clippy proofs | done | Both required proof commands exit 0 before commits |
| B3 await_holding_lock | partial | Clippy default warning applies; no dedicated seeded bar-run receipt |
| B3 IPC-reachable panic/unsafe/blocking/error-key audits | partial | Required rules and compiler lints; semantic audit execution absent |
| B4 shell-free/npm resolution/hidden tree stop | done | Existing hidden process proofs reexecuted |
| B4 atomic/versioned/crash-compatible records | partial | Existing journal and broker proofs plus legacy read tests; full G8 crash matrix is T5/T13 |
| B4 idle ≤250 MB and measured persistent processes | done | Hidden measurements pass; no new persistent process added |
| B4 credential folders untouched | done | New wire/broker/ground guards; existing sandbox audit reexecuted |
| B5 flagged command/capability inventory review | partial | Required bar rule, no actual command inventory change in this patch |
| B5 CSP/no remote scripts/content/origin IPC | partial | Existing shell checks; goal-linked receipt needed |
| B5 scoped Rust inputs/signatures/channel separation | partial | Existing native guards; goal-linked receipt needed |
| B5 webview CI or disclosed weaker/not-run | partial | Windows hidden suite runs; macOS screen is explicitly skipped by testkit |
| B6 lint/typecheck/build | done | Required native commands exit 0 |
| B6 props/reactivity/timer/listener cleanup | partial | Existing UI lint/tests; whole-goal audit needs runner linkage |
| B6 last-forty message window | partial | Existing UI behavior and long-thread hidden proof, not bar-bound |
| B6 five hand-authored locales | partial | Existing locale checks retained; native-speaker authorship is not inferred |
| B6 tokens/locked colors/no duplicate controls | partial | Existing checks and required rules; new whole-product review absent |
| B6 every screen at all three widths and all themes | missing | Existing suite covers selected flows, not the complete requested matrix |
| B7 rendered contrast/24px/keyboard/focus/motion/13–18px | partial | Existing contrast/keyboard/motion proofs, no complete G8 rendered audit |
| B8 OSV inventories and public name/version-only query | done | Real lockfile inventory and exact request payload test |
| B8 offline/unknown/advisory/malicious fixtures | done | Standard OSV response fixtures; synthetic IDs clearly labeled |
| B8 local Cargo/npm SPDX expression/coverage checks | done | Native parsers reject missing license and omitted locked package |
| B8 original secret scanner, Unicode safe, redacted | done | Real negative tests and source mutation |
| B8 no installs/telemetry/unauthorized destinations | done | No added dependency/tool; exact source grant and intent/outcome logging |
| B9 feature/bug reproduction/root-cause/callers | partial | Required rules; actual writer chronology/caller records are T5 |
| B9 refactor golden/API/schema/noise validation | done | Data-driven before-edit, exact-shape and measured-noise comparator |
| B9 native collector produces those base manifests/benchmarks | partial | Comparator is real; captured inputs require T5 proof execution |
| B9 dependency upgrade changelog/breaking tests | partial | Required rule; actual upgrade evidence not run on this branch |
| B10 empty/huge/Unicode/offline/timeout/two-window probes | partial | Existing/general negative tests; full G8 edge matrix absent |
| B10 contracts and saved schemas compatible/versioned | done | Active strict wire and legacy durable-record tests |
| B10 real first-token ≤2s | missing | No paid/live model timing claimed |
| B10 docs/locales and coherent English commit | partial | English commits/docs; no behavior text added to screen/locales |
| Software/map parity ≥95%, blind both-order/plain CLI probes | missing | T13 benchmark not run |
| G10 every required runnable proof on the real G8 goal | missing | No actual G8 mission/qualification in this branch |
| G1–G9, canary, rollback and benchmark retained at G10 | missing | T5/T11/T13 qualification remains required |

### Scope and phase verdict

The producer's complete-T5B claim is rejected. The pure compiler, receipt/cache guards and native B8 implementation are
substantially corrected and testable. This branch still cannot establish complete software delivery: an admitted T5
runner must consume these interfaces, T9 must execute the draft/ghost/full-render matrix, and T13 must run the actual
G8/G10 and benchmark. Those absences are open findings, not waived checks. No signing/release, installed Faber, live CLI
credentials, third-party install or paid comparison was used. No next phase or G10 is approved by this review.

### First fix: trust boundaries

Implemented full semantic comparison of mandatory library checks, protected critic replacement, framed candidate hashing,
validated cache insertion and digest invalidation, graph/owner/evidence/scope validation, Plan/wire/journal enforcement when
software metadata is present, nonempty/unique sheet pieces, strict receipt outcomes/artifacts, complete ordered phase/cost tags,
immutable compile revisions and explicit acceptance rejection for missing/failing/not-run/wrong-candidate outcomes.
Legacy durable Plans without additive metadata remain readable; their absence is never acceptance evidence.
The scanner no longer returns a secret line and avoids Unicode byte-boundary slicing.

Test-first counterexamples: 13 new tests failed on the respective pre-fix implementation; 15 regression tests now pass.
Real source mutations: 14 rules were exercised. Twelve mutations were killed in the first run. The owner/cycle mutations
initially survived because their input also lacked proof evidence; the inputs were fixed and both were rerun and killed.
Each mutation restores the original bytes in a finally block. The script and cumulative result JSON live beside this report.
No mutation result was counted as a proof of a live model/ghost/benchmark gate.

Validation before commit 1: npm lint/typecheck/build, fmt, both clippy commands, and cargo t exit 0. cargo t reports
804 passed, 0 failed, 0 not run, 0 skipped; 725 Rust tests. Hidden measurements include 107/110 MB idle,
184 MB after cycling themes, and 117.2 MB for the whole warm app tree. cargo t --all also exits 0: 887 passed.
The Node tools were missing from this worktree. An existing node_modules with an identical package-lock SHA-256
08D260DB6A1C73771022F58C530FE377FD213CB16ED3CA68310AD24F53C6343F was reused through a local ignored junction;
no package/tool was installed. All Rust commands use the requested isolated target and environment.

Native API design uses the official [OSV query contract](https://google.github.io/osv.dev/post-v1-query/),
[Cargo metadata contract](https://doc.rust-lang.org/cargo/commands/cargo-metadata.html), and
[npm registry documentation](https://docs.npmjs.com/cli/v11/using-npm/registry/). Unknown response shapes,
private registries, missing metadata and paginated/incomplete responses must never become a pass.

### Final source mutation run

37 source mutations ran sequentially and were killed by a runtime assertion/test failure. Compilation failures were not counted as killed mutants. Files were restored byte-for-byte after each mutation. The run made no network request: source authorization was mutated only in the pure URL/grant test.

| Rule | Targeted test | Result |
|---|---|---|
| mandatory semantics | `mandatory_checks_cannot_be_weakened` | killed |
| critic preservation | `critic_cannot_replace` | killed |
| digest inputs | `candidate_hash_has_unambiguous` | killed |
| cache validation | `cache_does_not_reuse` | killed |
| digest invalidation | `invalidation_also_removes` | killed |
| owner binding | `compiler_rejects_missing_owner` | killed |
| cycle guard | `compiler_rejects_cycles` | killed |
| evidence producer | `compiler_requires_evidence_producer` | killed |
| plan entry guard | `plan_cannot_carry_sheet` | killed |
| secret redaction | `secret_scanner_handles_unicode` | killed |
| receipt completeness | `receipt_rejects_empty` | killed |
| sheet completeness | `sheet_rejects_empty` | killed |
| compile phase completeness | `compile_receipt_rejects_missing` | killed |
| acceptance result | `acceptance_requires_each_check` | killed |
| per-piece coverage | `every_piece_receives_locked` | killed |
| ghost coverage | `project_screen_proofs::ghosts_require_every` | killed |
| draft width coverage | `project_screen_proofs::drafts_require_two` | killed |
| draft before code | `project_screen_proofs::drafts_require_two` | killed |
| ghost hidden app | `project_screen_proofs::ghosts_require_every` | killed |
| official source scope | `project_native::network_requests_are_scoped` | killed |
| source authorization | `project_native::network_requests_are_scoped` | killed |
| OSV unknown result | `project_native::osv_unknown_offline` | killed |
| registry exact version | `project_native::registry_requires_exact` | killed |
| license AND semantics | `project_native::license_metadata_checks` | killed |
| source input revalidation | `project_compile::durable_cache_reuses` | killed |
| private proof authority | `project_compile::acceptance_hashes_evidence` | killed |
| artifact hash verification | `project_compile::acceptance_hashes_evidence` | killed |
| refactor public API | `project_compile::refactor_requires_base` | killed |
| fast path risk | `fast_path_checks_existing` | killed |
| broker path boundary | `repair_ownership_does_not` | killed |
| finder isolation | `project_compile::software_library_miss` | killed |
| fenced heading | `grounds_do_not_treat` | killed |
| receipt reservation | `project_compile::acceptance_hashes_evidence` | killed |
| license purpose shortcut | `project_bar::spdx_license_checker_never` | killed |
| malformed Cargo inventory | `project_native::lock_inventory_covers` | killed |
| sheet extension preservation | `project_compile::sheet_records_keep` | killed |
| OSV public provenance | `project_native::osv_requires_public_registry` | killed |

Commit 1: `f804b08`, trust boundaries. All seven AGENTS.md proofs and `cargo t --all` exited 0 before that commit; full run: 887 passed, 0 failed/not-run/skipped. The second coherent patch adds native evidence, durable compilation/cache and record validation. Its frozen-source final proof run exits 0: cargo t reports 840 passed; cargo t --all reports 923 passed; both have zero failed, not-run or skipped checks.

### Frozen-source proofs before the second commit

All eight required commands exit 0: npm run lint, npm run typecheck, npm run build, cargo fmt --all --check, cargo clippy --workspace --all-targets -- -D warnings, cargo clippy -p faber --features test-hooks -- -D warnings, cargo t, cargo t --all. The complete run contains 761 Rust, 83 UI and 79 hidden screen tests. Idle memory is 108 MB and the warm whole app tree is 119.0 MB. The source remains byte-identical after the final 37 killed mutations. No installs or HTTP request occurred in the new native tests.

The latest selfdev base has ten additional budget/integration commits. A merge preview reports a single module-declaration conflict in engine/src/project.rs. Base integration and its required proofs/CI are the next step. This does not change the missing live G10 evidence.

### Base integration proofs

Integrated selfdev at 041da3b; the sole conflict retained both the bar and budget module declarations. All seven AGENTS.md proofs plus cargo t --all exit 0 on the combined source. Both cargo t and cargo t --all report 982 passed, zero failed/not-run/skipped. The warm whole app tree measures 119.3 MB. Fix commits: f804b08 (trust boundaries) and f657b5b (native evidence and durable compilation). The integration commit preserves both histories. PR and final Windows/macOS CI results will be recorded in the external report after publication.

### First CI result and portable fixture correction

Draft PR: https://github.com/startRED/faber/pull/8, base selfdev, head c47feff. macOS job 115135192357 failed grounded_additions_verify_repo_heading_exists; cargo stopped at that test crate and reported the Rust suite failure as well. Its result was 535 passed, 2 failed, 1 skipped, not a pass. The fixture used the macOS /var temporary-directory alias, while broker IO requires enrolled canonical roots and forbids links. The fix uses one canonical_tempdir helper in the existing test support module for all four bar/compiler/native fixture files. Production link checks remain unchanged. The existing failing macOS test is the red counterexample; all four affected test files now pass locally. All required proofs are rerun before committing this correction. The screen skip on macOS remains an explicit limitation, not G10 evidence.

Portable-fixture correction proofs: all eight commands exit 0; cargo t reports 899 passed and cargo t --all reports 982 passed, zero failed/not-run/skipped. The warm tree is 118.7 MB. The first Windows CI job passed all stages at c47feff (36m8s); the macOS failure remains recorded rather than rerun as an unexplained transient. The corrected fixture commit will receive fresh CI on both operating systems.

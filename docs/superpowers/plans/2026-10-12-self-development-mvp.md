# Self-development MVP implementation plan

Date: 2026-10-12. Status: proposed; implementation has not started under this request.
Spec: [Self-development MVP](../specs/2026-10-12-self-development-mvp-design.md).
Goal: Faber completes a proven local change to Faber and promotes its next development build by 2026-10-20 to 22.
R96 moves this slice ahead of Full Chat. The rest of Chat, catalog, powers, Livre, and public release automation wait.

## Execution rules

This plan describes future work. Do not execute it as part of the document-writing task.
Each numbered task is one coherent commit, made as soon as its proof passes.
Write the task's regression tests first, observe the relevant failure, then implement and prove.
Use existing Rust/tokio/serde, CLI drivers, Solid components, and testkit. No general workflow framework.
All code, comments, documents, and commits are English. All screen text is in the existing five locale files.
Hand-author locale entries under AGENTS.md; do not use machine translation.
Use only original code. Official CLIs run unmodified. Never inspect provider credential stores.
No visible windows/consoles, production checkout mutation, public release, or installed-release update during verification.
All real git/process integration runs belong to audited hidden scenarios, never unit tests.
Real-CLI capability/calibration/canary runs are separate explicit probes within a granted cost/daily allowance.
They are not `cargo t`, do not run automatically in CI, and do not gain paid-credit permission from this plan.

New files below are proposed implementation paths, not claims that those modules already exist.
Shared exports and schemas have one owner. Integration orders are serial even when development is parallel.

## Proof sets used by every task

Before any cargo command, set the maintainer's Rust paths in the same process environment:

```powershell
$env:RUSTUP_HOME = 'E:\Apps\rust\rustup'
$env:CARGO_HOME = 'E:\Apps\rust\cargo'
$env:PATH = 'E:\Apps\rust\cargo\bin;' + $env:PATH
```

G is the complete repository proof set, all exit 0 before the commit:

```text
npm run lint
npm run typecheck
npm run build
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p faber --features test-hooks -- -D warnings
cargo t
```

GA is G with `cargo t --all` instead of `cargo t`; use it for UI, shell, testkit, or unknown affected scope.
The executor resolves npm's real executable/script; no shell or `.cmd` launch is allowed.
Focused first-test commands below precede G/GA; they do not replace it.
For a clean candidate, drive test selection from recorded changed paths or run `cargo t --all` explicitly.
All screen scenarios use the hidden desktop and audit, with no fallback to the user's desktop.
Cross-vendor review gates every accepted source change; the writer never reviews its own change.

## Dependencies and parallel areas (R91)

| Task | Required tasks | Owned area | May be developed in parallel with |
|---|---|---|---|
| 1. Frozen contracts and journal | None | Project schemas, persistence, documentation | None until the contract is accepted |
| 2. Driver policy and lifecycle gate | 1 | Drivers, events, process lifecycle | 4 after shared schema freeze |
| 3. Live cost and daily grants | 2 | Metering, usage/budget/ledger, admission | 4; UI mock work after contract freeze |
| 4. Git broker and worktree leases | 1 | New project git/lease modules | 2/3 |
| 5. Lead and task dispatch | 2, 3, 4 | New mission runtime and engine hooks | 9 on contract mocks; 11 pure updater state |
| 6. Worker packet execution | 5 | Worker orchestration and artifacts | 9; 11 pure updater state |
| 7. Proof runner and merge queue | 6 | Project proof/integration modules | 9; 11 pure updater state |
| 8. Stop and recovery integration | 7 | Runtime recovery/shared engine hooks | 10 once shared UI schema is frozen |
| 9. Projects screen | 1; 8 before final integration | UI board/API/locales, shell command adapter | 3–8 against accepted mocks; never edit their engine areas |
| 10. R92 notifications | 9 | Tray policy/sound, Settings | 11 after shell-main changes have one owner |
| 11. Local slots and rollback helper | 1; 8/9 before integration | Local-update state/helper, startup adapter | 5–7 pure module work; 10 disjoint files only |
| 12. Headless next-build canary | 7, 8, 11 | Canary/run entry, update integration | None for shared startup wiring |
| 13. Real Seguro acceptance | 3, 8, 9, 10, 12 | Acceptance fixtures/report and targeted fixes | Read-only review/research on independent nodes |
| 14. Equilibrado | 4, 7, 13 | Remote broker and enrollment extension | 15 if paths/contracts are disjoint |
| 15. Qualified parallel writers | 3, 4, 6, 7, 13 | Writer admission/qualification | 14 under explicit area leases |

Task numbers define the recommended landing order. The dependency column defines when work can start.
Task 9's first dependency allows tests, UI mock work, and component implementation before engine completion.
Task 11's first dependency allows pure state-machine/helper work; startup wiring waits for 8/9.
Ready nodes start immediately within live grants, daily pace, RAM, and review capacity. Do not wait for a whole wave.
Only disjoint writer areas may overlap; shared `engine.rs`, `main.rs`, API, locale registries, and manifests have one owner.
Workers without source leases may research, review, and propose tests while a writer implements.
Tasks 14/15 are included optional extensions after the first usable Seguro gate, not prerequisites for the date.

## Task 1 — Freeze the minimum contracts and durable journal

Files: new `engine/src/project.rs`, `engine/src/project/store.rs`, `engine/tests/project.rs`;
`engine/src/lib.rs`, `engine/src/model.rs`, `engine/src/store.rs`, `engine/tests/ledger.rs`;
later implementation documentation in `docs/design.md` and the proposed roadmap section.

Tests first: old Project JSON loads without granting effects; repo/grant identity survives reload; graph rejects cycles,
missing IDs and duplicate deliverables; contract revision invalidates only descendants; failed rename/full disk keeps old state.
Corrupt safety data blocks admission; atomic-write failure returns a locale key; old readers preserve extension records.
Focused proof: `cargo test -p faber-engine --test project` and `cargo test -p faber-engine --test ledger`.

- [ ] Define repo/grant/task/contract/decision/lease/artifact/receipt shapes from the spec, with schema versions.
- [ ] Add `.faber/missions/<id>/` records and private runtime sidecars using fallible atomic writes.
- [ ] Return persistence errors for every safety ledger reached by Projects; never fall back to empty authority.
- [ ] Publish the fixed UI event/command and worker JSON contracts, including errors and examples.
- [ ] In this future task only, apply the agreed roadmap order proposal and minimum screen inventory update.
- [ ] Run G, receive other-vendor review, then commit `feat: define durable project missions`.

## Task 2 — Prove driver restrictions, termination, and invocation controls

Files: `engine/src/claude.rs`, `engine/src/codex.rs`, `engine/src/events.rs`, `engine/src/proc.rs`;
new `engine/src/project/policy.rs`, `engine/tests/project_policy.rs`;
`engine/tests/claude_session.rs`, `codex_session.rs`, `engine/tests/support/mod.rs`;
new `tests/fixtures/projects/policy-*.jsonl`, `testkit/src/screen/projects.rs`, its registration;
platform helper wiring/Cargo manifests only for the required installed Windows wrappers.

Tests first: explicit model/effort/permission on launch and resume; supported restrictions accepted, absent capability denied;
git push/reset/ref deletion, shell chains, native subagents, MCP/hook escape and linked `.git` common-dir writes denied;
interrupted result acknowledged; ignored interrupt kills owned descendants; reused PID and shared-server tasks remain safe.
Scripted unit tests use fake `Proc`; real process/metadata escapes run only in hidden testkit scenarios.
Focused proof: `cargo test -p faber-engine --test project_policy` plus both driver session test targets.

- [ ] Add role policy distinct from existing side-question `instructions`, which currently disables needed worker behavior.
- [ ] Keep official CLIs unchanged; configure per-session native-tool restrictions without global config edits.
- [ ] Use structured engine `read`/`patch`/`proof` requests; no general shell parser or new MCP server.
- [ ] Add explicit Claude git deny rules; Codex `workspace-write` for writers, read-only for analysis, no escalation.
- [ ] Handle capability/terminal-reason keys, graceful stop acknowledgment, tree termination and silence timeout.
- [ ] Establish an enforceable invocation exposure bound or verified live usage plus safe stop margin, including nested calls.
- [ ] Run separate bounded official-capability probes and record exact versions/controls/latency; no credentials or paid fallback.
- [ ] If restrictions or spend bounds cannot be proved, record the blocker and stop automated-role rollout.
- [ ] Run GA, cross-vendor review, commit `feat: constrain project CLI execution`.

Task 2 follow-up evidence (2026-10-12, `selfdev-t2`):

- Baseline implementation: `11c798c`. All automated roles still fail admission before spawning; Chat/manual
  Projects launch defaults are preserved. Cross-vendor review findings, corrections, controls and raw bounded
  evidence are in [the research report](../../research/2026-10-12-project-policy-probes.md) and its JSON sibling.
- Review regressions cover completion without `turn/started`, sends during interrupt grace, customization
  switches, installed Claude flags, all nine fake-wire escape requests, and unconditional admission denial.
  The review's no-Codex-shell-switch claim and proposed `on-request` containment fix are false.
- Windows tree blocker is closed: assign the trusted host guard to a kill-on-close job before the CLI can start.
  Hidden fake tests require the guard/CLI/child/grandchild to disappear on stop, ownership drop and host crash;
  stale creation-time/PID records cannot kill a live peer. PID reuse is simulated deterministically, not forced.
  Non-Windows retains its existing child path and tree blocker.
- R103 owner authorization granted a separate six-slot subscription experiment, not unattended admission.
  The hidden opt-in example consumed all six slots (fourteen one-escape prompts); slot 4 failed before a prompt
  and was not retried. Claude 2.1.296/haiku low exposed no tools and left all six artifacts unchanged.
  Codex 0.162.1/gpt-6-luna low initially wrote outside via implicit TEMP roots; disable shell/subagent features,
  implicit TEMP roots and network. Its tightened final session left all six tested artifacts unchanged.
- Model-reported EPERM accompanies failed native-code items; independently checked artifacts are the evidence.
  An absent unapproved MCP tool is not hostile-config containment proof. Linked common git metadata, read scope,
  configured customizations and resumed policy remain unproved. Never unlock from these bounded observations.
- Exposure remains blocked: cumulative usage was recorded, but no hard input/output/nested-call bound or real
  interrupt-latency sample exists. All successful turns completed before the probe deadline; elapsed turn time
  is not stop latency. No guessed token price, daily headroom or safe stop margin is introduced (Task 3 owns grants).
- Spec issue: section 5 requires restrictions even for automated read-only roles, while section 13's read-only
  fallback is ambiguous. Keep saved/local planning available; deny unverified automated CLI roles.
- Shared file inventory and review disposition are in the research report. Git broker/lease files were untouched.
- Follow-up GA: all seven required proof commands exit 0; `cargo t --all` reports 590 passed, no failures/skips.
  The Windows fake tree scenario removes four owned processes in each mode and preserves the stale-PID peer.

Second review follow-up: [Opus disposition](../../research/2026-10-12-task-2-opus-review.md).
Manual sessions have no automatic silence interrupt; role watches and explicit stops remain bounded.
Provider pinning matches the probe, role resume IDs are validated, guard acknowledgement is asynchronous,
the cache example dispatches guard mode, and reordered Codex completions require a matching RPC identity.
Both role loops replay native-denial fixtures. Job proofs cover inherited members; delegated WMI/task/COM
launches, read/network scope, hostile config, mention expansion and real resume policy remain prerequisites.
No additional model session is authorized or executed in this round; all automated roles remain denied.
Second-review GA: all required proofs exit 0; full run 602 passed, no failures/skips. The EOF regression was
reproduced with a fake child and fixed through forwarding-future ownership. Warm guard 2.0 MB, warm tree 117.2 MB.

## Task 3 — Add priced live grants and the hard daily gate

Files: `engine/src/usage.rs`, `engine/src/budget.rs`, `engine/src/ledger.rs`, `engine/src/signals.rs`,
`engine/src/engine/auto.rs`, `engine/src/engine/pulse.rs`;
new `engine/src/project/budget.rs`, `engine/src/project/prices.json`, `engine/tests/project_budget.rs`;
`engine/tests/turn_usage.rs`, `engine/tests/pacing.rs`, new usage/daily fixtures in `tests/fixtures/projects/`.

Tests first: uncached/read/write/output priced once; Claude cumulative/nested calls; Codex resume/tool totals;
late/missing counters reserve exposure; aborted turn does not reset spend; duplicate updates settle once;
simultaneous grants do not overbook; lead/reviewer/pulse/canary cost counts; cap shrinks under outside use;
day/reset/clock anomalies and stale billing fail closed; restart never grants a second daily allowance;
paid fallback/unknown price denied; safe stop occurs before all admitted exposure can exhaust the cap.
Focused proof: `cargo test -p faber-engine --test project_budget` and `cargo test -p faber-engine --test turn_usage`.

- [ ] Extend existing settlement; do not rebuild quota-plan Task 1.
- [ ] Add dated published-rate accounting and complete/partial cost events with stable turn/invocation identities.
- [ ] Keep account points separate; implement reset-aligned daily room and conservative calibration evidence.
- [ ] Allocate daily/mission/worker grants and review/canary reserves; make estimates advisory only.
- [ ] Wrap all dispatch/continuations and live updates; unknown inflight cost stays reserved after stop/restart.
- [ ] Apply the same global allocation to overlapping Chat and Projects usage; no independent free budget.
- [ ] With missing calibration use a bounded already-granted read-only setup task, or park; never an uncapped probe.
- [ ] Run G, cross-vendor review, commit `feat: enforce live project spending grants`.

## Task 4 — Add the shell-free git broker and exclusive leases

Files: new `engine/src/project/git.rs`, `engine/src/project/lease.rs`, `engine/tests/project_git.rs`,
`engine/tests/project_lease.rs`; existing `engine/src/cli.rs` resolver as needed;
`testkit/src/screen/projects.rs`, `testkit/src/sandbox.rs`, new scripted git fixtures.
Shared exports belong to Task 1's owner.

Tests first: canonical common-dir identity; non-git/bare/no-base rejection; refs/arguments reject injection;
dirty owner checkout untouched; lease conflicts across case, parent paths, junctions and renames;
stale generation rejected; live process lease cannot expire into reassignment; revocation prevents next effect;
compare-and-swap refuses stale heads; no destructive cleanup or branch deletion.
Unit tests inspect argv/receipts with a fake executor. Hidden scenarios use temporary real repositories only.
Focused proof: `cargo test -p faber-engine --test project_git` and `cargo test -p faber-engine --test project_lease`.

- [ ] Validate enrollment, grant revision and author identity. Default to Seguro, never implicitly grant it.
- [ ] Create detached writer/candidate worktrees and `faber/<mission-id>` without switching the owner's checkout.
- [ ] Persist leases atomically; implement scoped hash-checked file operations and diff audit.
- [ ] Use explicit git argv; keep hooks/credential prompts from becoming an unattended execution escape.
- [ ] Preserve worktrees/artifacts on conflict, dirty state, unknown termination, or failed cleanup.
- [ ] Run GA, cross-vendor review, commit `feat: isolate local project git work`.

## Task 5 — Add the lead and event-driven ready dispatch

Files: new `engine/src/project/lead.rs`, `engine/src/project/run.rs`, `engine/tests/project_run.rs`;
`engine/src/engine.rs` and `engine/src/engine/auto.rs` hooks; lead JSON fixtures in `tests/fixtures/projects/`.

Tests first: malformed/cyclic lead plan denied; contract-ready engine/UI/test branches dispatch without wave waits;
critical path prioritized under one available slot; each of the four brakes explains waiting;
owner decision parks only descendants; mode switch never auto-approves money/legal actions;
contract revision invalidates exact dependents; failure leaves independent ready nodes running.
Focused proof: `cargo test -p faber-engine --test project_run`.

- [ ] Give the lead a fresh bounded session and short repo/rule map, then validate its structured plan.
- [ ] Keep normal/autonomous and git modes distinct; persist authored decisions and plan amendments.
- [ ] Recompute readiness after state/capacity changes; no polling fleet or arbitrary batch barriers.
- [ ] Schedule using existing Auto through the stronger Projects admission gate.
- [ ] Run G, cross-vendor review, commit `feat: dispatch ready project tasks`.

## Task 6 — Execute short worker briefs and artifact handbacks

Files: new `engine/src/project/worker.rs`, `engine/tests/project_worker.rs`;
`engine/src/project/run.rs` narrow hooks; `tests/fixtures/projects/worker-*.jsonl`;
`engine/tests/support/mod.rs` only under one fixture-owner lease.

Tests first: one worktree per writer; readonly test proposal cannot mutate tests; broker path/hash/lease checks;
worker cannot spawn peers or enlarge its grant; unrelated context is absent; related repair preserves failures;
cap/interrupt returns artifact and unfinished checks; oversize/stale packet denied; retries cannot reset totals.
Focused proof: `cargo test -p faber-engine --test project_worker`.

- [ ] Dispatch a written brief with fixed contract, scope, source references, proof IDs and cost grant.
- [ ] Implement bounded file-request/patch/proof/result envelopes between official CLI turns.
- [ ] Keep transcripts and session IDs private; send the lead summary plus artifact references.
- [ ] Start v0 with read-only fan-out and one source writer; reserve review capacity before writing.
- [ ] Run G, cross-vendor review, commit `feat: run scoped project workers`.

## Task 7 — Run contained proofs and the serial merge queue

Files: new `engine/src/project/proof.rs`, `engine/src/project/queue.rs`, `engine/tests/project_queue.rs`;
`testkit/src/gate.rs`, `contain.rs`, `screen/projects.rs`, `testkit/src/plan.rs` as needed;
`engine/src/project/run.rs` adapter; queue/reviewer fixtures in `tests/fixtures/projects/`.

Tests first: exact composed-tree commands before review; missing/failed proof prevents commit;
other vendor mandatory; stale head/tree/grant invalidates landing; changed repair reruns both gates;
conflicts preserve artifacts; clean candidate still selects changed tests; no double commit after uncertain receipt;
candidate script cannot read credentials, write owner files, spawn visible UI, or bypass outbound containment.
Focused proof: `cargo test -p faber-engine --test project_queue`, then project hidden scenarios.

- [ ] Resolve Faber's complete proof set into executable/argv definitions; reject arbitrary command text.
- [ ] Extend the existing audited runner with actual proof write/network containment, not only dead proxies.
- [ ] Compose one candidate at current own-branch head; keep one heavy proof active.
- [ ] Request fresh other-vendor review of exact tree and connected code; route repairs to one owner.
- [ ] Engine commits only after proof/review; fast-forward/CAS own branch; retain intent and receipt.
- [ ] Run GA, cross-vendor review, commit `feat: accept changes through a serial merge queue`.

## Task 8 — Prove mission stop, crash, and effect recovery

Files: `engine/src/project/run.rs`, `project/store.rs`, `project/queue.rs`, `project/budget.rs`;
new `engine/tests/project_recovery.rs`; `engine/src/engine.rs` lifecycle hooks;
`testkit/src/screen/projects.rs`, crash/ambiguous-effect fixtures.

Tests first: pause during turn/proof/review/ref update; admission closes immediately; no late quota timer dispatch;
crash before and after each intent/receipt; orphan descendants handled; PID reuse rejected;
unknown cost/commit/push parked rather than repeated; no premature lease release; round-trip state stays compatible.
Focused proof: `cargo test -p faber-engine --test project_recovery`.

- [ ] Reconcile processes, common-dir locks, worktree state, grants, refs and receipts before resume.
- [ ] Preserve useful artifacts, blocker reasons and next actions; scope failure does not cancel the mission.
- [ ] Ensure failed durable writes stop effects and stop itself does not mark tasks done.
- [ ] Run GA, cross-vendor review, commit `feat: recover interrupted project missions`.

## Task 9 — Add the minimal Projects screen and fixed shell commands

Files: new `ui/Projects.tsx`, `ui/projects.ts`; `ui/App.tsx`, `api.ts`, `styles.css`, `Running.tsx`, `Ask.tsx`;
`ui/locales/en.ts`, `pt-BR.ts`, `de.ts`, `es.ts`, `fr.ts`;
`src-tauri/src/main.rs` command adapter; `tests/ui/projects.check.ts`, `tests/ui/index.ts`;
`testkit/src/screen/projects.rs` and registration; implementation screen inventory in `docs/design.md`.

Tests first: failed grant/start keeps goal; ready/running/review/done/blocked states; stopped run resumes;
unknown versus measured cost; separate shared quota; stale event ignored; no blank success on refused command;
keyboard start/stop/decision/details; focus never stolen; all keys in every locale;
720/900/1100/1440/1920 px, 18 px text, themes and reduced motion; idle limit and hidden-window audit.
Focused proof: `cargo t --suite ui --filter projects` and `cargo t --suite screen --filter projects`.

- [ ] Start UI work on contract mocks as soon as Task 1 is accepted; do not wait for engine implementation.
- [ ] Reuse composer, controls, Ask/Running, tokens and stage; no theme/design dependency.
- [ ] Add typed commands for enrollment/grant, mission start/state/mode, stop/resume and decision answer.
- [ ] Replace unsafe direct project sends for MVP missions; workers never use the old owner-checkout cwd path.
- [ ] Keep diff/proof/technical details behind a click and use lists at narrow widths.
- [ ] Integrate against Tasks 5–8, run GA, cross-vendor review, commit `feat: show self-development projects`.

## Task 10 — Complete R92 needs-you notifications

Files: `src-tauri/src/tray.rs`, `tray/decide.rs`; new `src-tauri/src/notify.rs`, `src-tauri/tests/notify.rs`;
`engine/src/model.rs`/settings merge under shared-schema owner; `ui/Settings.tsx`, `api.ts`, all five locales;
first-party sound asset under `src-tauri/`; `tests/ui/settings.check.ts`, hidden notification scenarios.

Tests first: focused window emits nothing; fullscreen/game emits sound only; background emits both;
independent switches; unknown foreground suppresses popup; restart deduplicates same decision;
completion/automatic wait/promotion produces no notice; any notification never calls show/focus.
Focused proof: `cargo test -p faber --test notify` and `cargo t --suite ui --filter settings`.

- [ ] Extend existing needs-you watcher; keep stable decision identity across restarts.
- [ ] Add original sound, foreground/fullscreen policy and fallible native delivery.
- [ ] Keep notification strings in locales, including startup fallback text; remove reached hard-coded English fallback.
- [ ] Run GA, cross-vendor review, commit `feat: notify only for project decisions`.

## Task 11 — Add local build slots and an independent rollback helper

Files: new `engine/src/project/update.rs`, `engine/tests/local_update.rs`;
new `src-tauri/src/local_update.rs`, `src-tauri/src/bin/faber-local.rs`, `src-tauri/tests/local_update.rs`;
`src-tauri/src/main.rs`, `src-tauri/Cargo.toml`; hidden update scenarios; minimal update status in Projects/locales.

Tests first: slot paths reject installed release/aliases; wrong tree/hash cannot promote; old/new state round trips;
window exists or task/proof/git effect active blocks swap; open request cancels quiescence;
crash before/after manifest replacement, missing 30-second acknowledgment, five-minute early failure;
rollback once to last good, quarantine bad next, and no installer/window/console/public updater calls.
Focused proof: `cargo test -p faber-engine --test local_update` and `cargo test -p faber --test local_update`.

- [ ] Implement pure slot/health/transition state first in parallel with the coordinator after Task 1.
- [ ] Add trusted local helper that can monitor and restart without the replaced app alive.
- [ ] Use separate dev root/data, additive compatible state, retained last good and atomic manifest.
- [ ] Add explicit development channel selection; release-mode local builds never enter the public installed updater path.
- [ ] Wire headless startup and self-update trust after recovery/UI integration; never touch `E:\Apps\Faber`.
- [ ] Run GA, cross-vendor review, commit `feat: stage and roll back local development builds`.

## Task 12 — Build next and run the headless self-development canary

Files: `engine/src/project/update.rs`, `project/proof.rs`; `src-tauri/src/local_update.rs`, `main.rs`;
new `testkit/src/screen/local_update.rs` and registration;
new `tests/fixtures/projects/canary/` miniature repo source and fake transcripts;
`engine/tests/local_update.rs`, `src-tauri/tests/local_update.rs`.

Tests first: accepted tree only; proofs fail before build/canary promotion; exact executable/assets hash retained;
next fixes tiny failing test through lead/writer/proof/other-vendor/landing; no recursive self-update;
canary cost exhausts allowance into a saved wait; old data still usable after failed canary and rollback.
Focused proof: local_update unit targets and `cargo t --suite screen --filter local_update`.

- [ ] Queue only accepted self-changes; coalesce obsolete next builds, never skip latest-tree validation.
- [ ] Build next without an installer into its inactive slot through contained named commands.
- [ ] Run offline fake-CLI canary in GA; add separately budgeted real-CLI canary entry.
- [ ] Require successful real canary before live local promotion; preserve ready build while quota/reviewer is unavailable.
- [ ] Run GA, cross-vendor review, commit `feat: canary the next self-development build`.

## Task 13 — Prove the usable Seguro loop and close the date gate

Files: project/recovery/update tests and fixtures touched by discovered defects; `testkit/src/screen/projects.rs`,
`screen/local_update.rs`; new `docs/research/2026-10-20-self-development-acceptance.md` with actual measured date;
`docs/roadmap.md` status/legacy-gap rows only after evidence, not before.

Tests first: scripted small Faber goal has a red acceptance check, local landing, persisted report and promoted build;
forced early crash restores last good; post-rollback Resume consumes existing grants once; original checkout/stable install hashes unchanged.
Add the smallest failing regression for each real defect before fixing it. Do not expand product scope.
Focused proof: project/recovery/local-update targets and their hidden scenarios; final GA.

- [ ] With granted quota, run one small real Faber goal through both vendors in Seguro.
- [ ] Build/canary/promote at no-window quiescence; verify the accepted revision is the one running.
- [ ] Perform bounded rollback drill and resume; record cost completeness, peak RAM, timings and exact receipts.
- [ ] Report time/token savings as unknown without paired evidence; make no unmeasured performance claim.
- [ ] Mark spec G1–G8 passed only with evidence; missing controls keep R96 incomplete.
- [ ] Run GA, cross-vendor review, commit `test: prove the self-development MVP`.

## Task 14 — Add Equilibrado push and draft PR under the project grant

This task follows usable Seguro. It does not delay the local target date.
Files: `engine/src/project/git.rs`, `project/queue.rs`, new `engine/tests/project_remote.rs`;
enrollment UI/API/locales, scripted git/gh remote fixtures, hidden project scenarios.

Tests first: Seguro never runs remote effects; explicit Equilibrado grant/remote/account match;
secret flagged before push; non-forced own-ref only; diverged/protected ref blocks;
unknown push/PR result reconciles without duplicate; grant revoked before dispatch blocks; no ready/merge/release action.
Focused proof: `cargo test -p faber-engine --test project_remote`.

- [ ] Add shell-free git/gh argv and idempotent intent/receipt reconciliation.
- [ ] Push exact proven own branch and create/reuse one draft PR; preserve local success on remote failure.
- [ ] Test against fake remote services, never the maintainer's real GitHub in automated tests.
- [ ] Run GA, cross-vendor review, commit `feat: publish granted project drafts`.

## Task 15 — Admit qualified parallel writers on disjoint areas

This task follows Studio v0 and evidence collection. It does not delay the local target date.
Files: `engine/src/project/run.rs`, `lease.rs`, `budget.rs`, `worker.rs`; new `engine/tests/project_parallel.rs`;
parallel/stale-contract fixtures; Projects reason/measurement display; measured paired report in `docs/research/`.

Tests first: qualified disjoint writers start immediately; unqualified classes stay serial;
parent/case/rename/shared schema collision orders writers; contract revisions stop affected branches;
review backlog/RAM/inflight exposure narrows width; high prediction error/rework makes only that class serial.
Focused proof: `cargo test -p faber-engine --test project_parallel`.

- [ ] Use the Studio measurement gate; require live cap/containment and low-error class evidence first.
- [ ] Compare identical compact serial/concurrent backlog with fixed versions, acceptance and rates.
- [ ] Include all attempts, coordination and review; insufficient confidence leaves qualification off.
- [ ] Keep equality of item-sum cost as target; no +10% spending permission or fixed speed promise.
- [ ] Run GA, cross-vendor review, commit `feat: admit proven parallel project writers`.

## Calendar and stop condition

| Window | Intended result |
|---|---|
| 12 October | Spec and plan only; no implementation under the current request |
| 13–14 October | Tasks 1–3 feasibility; start Task 4 once schema accepted; expose capability/calibration blockers early |
| 15–17 October | Tasks 4–8; Task 9 on contract mocks and Task 11 pure state can already advance |
| 18–19 October | Integrate Tasks 9–12, complete R92, recovery, compatibility and hidden rollback proofs |
| 20–22 October | Task 13 bounded real goal/canary, rollback drill, necessary fixes; Faber can begin Full Chat work |
| After usable gate | Task 14 remote drafts; Task 15 only as evidence qualifies writer classes |

The critical chain is contracts, driver feasibility, live grants, workers, queue, recovery, update canary, real acceptance.
Do not add the catalog, a generic tool server, a design overhaul, or release automation to that chain.
If hard spending or role containment is unprovable, stop automated execution and retain read-only planning.
If proofs or canary fail, retain current and the unfinished task. A date never overrides the gate.
The final implementation report must distinguish source tests, hidden integration proof, real-CLI evidence, and checks not run.

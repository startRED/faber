# Self-development MVP implementation plan

Date: 2026-10-12. Status: shortened plan; Task 1 is done, Tasks 2 and 4 are in review.
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
No visible windows/consoles, production checkout mutation, public release, or installed-release update during
verification.
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

Nine tasks replace fifteen. Six need implementation; two still need review. Keep the original IDs for branch and review
references.
Task 1 is done. Task 2 is in review on `selfdev-t2`; Task 4 is in review on `selfdev-t4`.
Their scope below stays unchanged. An in-review dependency is not done.
Task 5 absorbs Tasks 6, 7 and 8. Task 11 absorbs Task 12. Tasks 14 and 15 move to Later.
Reuse accepted work from `phase-1-base` and `notify-sound`; do not rebuild it.

| Task | Required tasks | Owned area | May be developed in parallel with |
|---|---|---|---|
| 1. Frozen contracts and journal (done) | None | Project schemas, persistence, documentation | Contract is frozen |
| 2. Driver policy and lifecycle gate (in review) | 1 | Drivers, events, process lifecycle | 4 after shared schema freeze |
| 3. Mission budget hook | 1; accepted Auto quota pacing from phase-1-base | Project admission/settlement adapter | 2/4; 9 on contract mocks |
| 4. Git broker and worktree leases (in review) | 1 | Project git/lease modules | 2/3 |
| 5. Mission runner, queue and recovery | 2, 3, 4 | Runtime, workers, proof/integration, recovery | 9 on contract mocks; 11 pure update state |
| 9. Projects screen | 1; 5 before final integration | UI board/API/locales, shell command adapter | 3/5 on mocks; 11 disjoint pure state |
| 10. Mission notification wiring | 1; accepted notify-sound 158566b; 5/9 before integration | Mission needs-you adapter | 11 with one owner for shell wiring |
| 11. Self-update and canary | 1; 5/9 before integration | Existing updater adapter, rollback, canary | 5/9 pure state; 10 disjoint files |
| 13. Real Seguro acceptance | 3, 5, 9, 10, 11 | Acceptance report and targeted fixes | Read-only work on independent nodes |

Task numbers suggest landing order. Start any task whose dependencies are done (R91).
Tasks 9/10 can use accepted contract mocks now. Task 11 can develop pure update state now.
Their final integration waits for the listed runtime dependencies.
Ready nodes start within live grants, daily pace, RAM, and review capacity. Never wait for a whole wave.
Only disjoint writer areas may overlap. Shared engine, shell, API, locale registries, and manifests have one owner.
MVP automated missions use read-only fan-out and one source writer. Parallel writer qualification is Later.

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

## Task 3 — Connect mission grants to existing Auto pacing

Files: new `engine/src/project/budget.rs`, `engine/tests/project_budget.rs`;
existing Auto/usage/ledger hooks only where mission attribution needs them.
Reuse `phase-1-base` budget/ledger/usage, the daily 1/7 line, and API-equivalent accounting.
Do not add another price table, quota meter, daily clock, or settlement ledger.

Tests first: concurrent mission reservations cannot overbook the shared allowance;
lead/worker/reviewer/repair/canary usage settles once; late/unknown cost stays reserved across stop/restart;
outside use or a smaller cap closes admission; missing rates/calibration/billing never grants free work.
Focused proof: `cargo test -p faber-engine --test project_budget` plus existing pacing/usage regressions.

- [ ] Add only mission/worker attribution, live caps, and review/canary reservations to the existing gate.
- [ ] Check every dispatch and continuation; settle stable invocation IDs through the existing usage path.
- [ ] Share the allocation with Chat. Estimates remain advisory; unknown exposure remains reserved.
- [ ] Reuse reset/staleness/paid-fallback rules. Missing evidence parks work or permits only bounded already-granted
  setup.
- [ ] Run G, cross-vendor review, commit `feat: connect mission spending grants`.

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

## Task 5 — Run missions through briefs, proofs, queue and recovery

This is the first task that spawns automated roles. It includes former Tasks 5–8.
Files: new `engine/src/project/{lead,run,worker,proof,queue}.rs`;
`engine/tests/project_{run,worker,queue,recovery}.rs`; narrow engine/Auto hooks;
existing project store/budget; `testkit/src/{gate,contain,plan}.rs`, `screen/projects.rs`;
lead/worker/reviewer/recovery fixtures in `tests/fixtures/projects/`.

Tests first: malformed/cyclic plans, stale contracts/packets and oversized requests fail closed;
all eligible ready nodes start without waves; four resource brakes explain waits; decisions park only dependents;
leases/path/hash checks reject escapes; workers cannot spawn peers, mutate tests read-only, or enlarge grants;
exact composed-tree proof precedes fresh other-vendor review; repairs repeat both; missing review blocks landing;
stale head/tree/grant refuses CAS; conflicts preserve artifacts; clean candidates select changed tests;
stop/crash at every intent/receipt boundary preserves work, unknown spend and live leases without duplicate effects.
Focused proof: the four project test targets above, then audited hidden project scenarios.

- [ ] First close Task 2's unproven containment, owned-tree termination and invocation-exposure proofs. Test
  native/MCP/hook/subagent/metadata escapes, PID reuse, shared servers, outbound/write denial and hidden candidate
  scripts. Record bounded official-CLI capability/calibration evidence. No automated role starts until these pass;
  unsafe controls block rollout.
- [ ] Give the lead a fresh bounded context and validate its plan. Persist authored decisions and amendments; keep
  autonomous mode separate from git mode. Defaults never approve money/legal effects.
- [ ] Recompute readiness on completion, grant, decision or lease changes. Use existing Auto with Projects admission,
  critical-path priority and aging; no polling fleet or wave barrier.
- [ ] Send short briefs with contract, scope, base, references, proof IDs, grant and stop condition. Use bounded
  read/patch/proof/result envelopes; keep transcripts/session IDs private and return summary plus artifact references.
- [ ] Start read-only fan-out and one source writer, each writer in its leased worktree. Reserve reviewer capacity
  first; keep retry/repair/diagnostic totals within the same caps.
- [ ] Resolve named proofs to executable/argv. Use actual audited write/network containment; candidate scripts cannot
  access credentials, owner files or the visible desktop. No implicit installs.
- [ ] Compose at the current own-branch head. Run one heavy proof at a time, then fresh other-vendor review of that
  exact tree and connected code. One repair owner repeats proof/review after changes.
- [ ] Commit only proven accepted paths, then CAS/fast-forward the own branch. Persist intent and receipt; stale heads
  recompose. Never substitute writer claims for proof.
- [ ] Stop closes admission and quota timers, checkpoints and interrupts, then kills only owned descendants after grace.
  Cancel promotion; release no lease/reserve before termination and settlement are known.
- [ ] Reconcile processes, locks, worktrees, refs, grants and receipts before resume. Park uncertain effects instead of
  replaying; durable-write failures halt effects. Unfinished work never becomes done; independent branches continue.
- [ ] Run GA, cross-vendor review, commit `feat: run recoverable project missions`.

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
- [ ] Integrate against Task 5, run GA, cross-vendor review, commit `feat: show self-development projects`.

## Task 10 — Wire mission decisions into the R92 notifier

Reuse `notify-sound`, commit `158566b`. Its sound, foreground policy, delivery and Settings switches already exist.
Files: mission event/needs-you adapter and existing `src-tauri/src/tray.rs` watcher;
project fixtures and hidden notification scenarios; locales only for new mission decision text.

Tests first: stable mission decision deduplicates across restart; focused means silence;
fullscreen/game means sound only; background respects both switches; uncertain detection suppresses popup;
completion, proof, commit, quota wait and promotion emit nothing; never show/focus a window.
Focused proof: existing tray tests and hidden mission notification scenarios.

- [ ] Map only concrete choices, missing permission and unresolved direction/money/legal blockers to existing needs-you
  events.
- [ ] Keep stable decision identity through reload and reuse existing locale/delivery paths. No new sound or Settings
  work.
- [ ] Run GA, cross-vendor review, commit `feat: wire mission decision notifications`.

## Task 11 — Reuse the updater for local self-update and rollback

This includes former Task 12. Reuse `src-tauri/src/main.rs` `fetch_update`, `.github/workflows/release.yml`
and `latest.json` for existing artifact/signature/platform handling. Do not rebuild the release updater.
Reuse its safe staging/lifecycle pieces for the explicit development channel; public release publication stays out.
The current Windows install-on-exit path launches an installer. It cannot serve a no-window local swap unchanged.
Add only the local exact-tree/canary gate, headless transition and one previous-build rollback it lacks.

Files: small project update state and shell updater/startup adapter; first-party rollback helper only where independent
restart is required;
`engine/tests/local_update.rs`, `src-tauri/tests/local_update.rs`, `testkit/src/screen/local_update.rs`;
`tests/fixtures/projects/canary/`; minimal Projects update status/locales.

Tests first: canonical paths/aliases reject installed release; only accepted exact trees/hashes promote;
old/new data round trips; active window/turn/proof/git effect blocks swap; open request cancels quiescence;
headless next fixes the tiny failing test through writer/proof/other-vendor/landing without recursive updates;
crash at transition boundaries, absent 30-second health acknowledgment or five-minute probation failure restores
previous once;
rollback/resume never resets spend, loses data, loops restarts, opens UI/console, or invokes the public installer.
Focused proof: both local_update targets and `cargo t --suite screen --filter local_update`.

- [ ] Use separate development root/data and explicit channel; never infer it only from debug mode or touch
  `E:\Apps\Faber`.
- [ ] Stage the latest accepted exact tree as next through contained build commands. Coalesce obsolete builds; retain
  hashes and proof receipts. Never overwrite a running executable.
- [ ] Run the frozen tiny-repo fake-CLI canary in GA and a separately granted real-CLI canary before live promotion.
  Reserve today's cost; unavailable quota/review leaves next ready and waiting.
- [ ] Require self-update trust, green proof/canary and rollback-compatible saved data. Additive fields or versioned
  sidecars only; no destructive migrations.
- [ ] At no-window quiescence, flush state and settle processes. Record an atomic transition and retain current as the
  one immutable previous build. Reuse updater pieces; no parallel download/signing/release pipeline.
- [ ] An independent trusted helper restarts next headlessly, reconciles interrupted transitions, and monitors health
  for 30 seconds plus five-minute probation. Early failure rolls back once and quarantines next; helper changes apply
  only to future transitions.
- [ ] Resume saved work only after health. A user open request during swap waits for health; only that request opens a
  window.
- [ ] Run GA, cross-vendor review, commit `feat: gate local self-update with canary and rollback`.

## Task 13 — Prove the usable Seguro loop and close the date gate

Files: project/recovery/update tests and fixtures touched by discovered defects; `testkit/src/screen/projects.rs`,
`screen/local_update.rs`; new `docs/research/2026-10-20-self-development-acceptance.md` with actual measured date;
`docs/roadmap.md` status/legacy-gap rows only after evidence, not before.

Tests first: scripted small Faber goal has a red acceptance check, local landing, persisted report and promoted build;
forced early crash restores last good; post-rollback Resume consumes existing grants once; original checkout/stable
install hashes unchanged.
Add the smallest failing regression for each real defect before fixing it. Do not expand product scope.
Focused proof: project/recovery/local-update targets and their hidden scenarios; final GA.

- [ ] With granted quota, run one small real Faber goal through both vendors in Seguro.
- [ ] Build/canary/promote at no-window quiescence; verify the accepted revision is the one running.
- [ ] Perform bounded rollback drill and resume; record cost completeness, peak RAM, timings and exact receipts.
- [ ] Report time/token savings as unknown without paired evidence; make no unmeasured performance claim.
- [ ] Mark spec G1–G8 passed only with evidence; missing controls keep R96 incomplete.
- [ ] Run GA, cross-vendor review, commit `test: prove the self-development MVP`.

## Later — outside the MVP

- Former Task 14: Equilibrado. Exact granted own-ref non-forced push and one draft PR; secret scan, intent/receipt
  reconciliation, no target merge or release. Seguro always denies remote effects.
- Former Task 15: qualified parallel writers. Disjoint leases plus measured Studio containment/cost/RAM/review gates;
  paired equal-acceptance evidence includes failed spend and coordination. Insufficient evidence keeps writers serial;
  10% comparison tolerance grants no extra spending.

## Calendar and stop condition

| Window | Intended result |
|---|---|
| 12 October | Task 1 done; Tasks 2/4 in review; shortened docs only under this request |
| 13-14 October | Finish reviews; small Task 3 hook; Task 5 first closes containment/termination/exposure feasibility; Tasks 9/10 mocks and 11 pure state start when ready |
| 15-17 October | Task 5 mission runner, queue and recovery; Task 9 board advances against contracts |
| 18-19 October | Integrate Tasks 9/10/11; prove notification wiring, exact-tree canary, compatibility and hidden rollback |
| 20-22 October | Task 13 real Seguro goal, canary, promotion, rollback/resume drill and necessary fixes; begin Full Chat only after acceptance |
| After usable gate | Later list only; neither remote drafts nor parallel writer qualification is in the MVP |

The critical chain is accepted drivers/git, mission grants, recoverable runner, self-update canary, real acceptance.
R91 applies throughout. Dates never hold an eligible ready task behind a batch.
Do not add catalog, a generic tool server, design overhaul, or public release automation.
If spending or containment cannot be proved, stop automated execution and retain read-only planning.
If proofs or canary fail, retain current and unfinished work. A date never overrides a gate.
The final report distinguishes source tests, hidden integration proof, real-CLI evidence, and checks not run.

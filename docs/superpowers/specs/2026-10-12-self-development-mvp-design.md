# Self-development MVP

Date: 2026-10-12. Status: proposed design for decision R96.
Target: useful on 2026-10-20 to 22, subject to the safety gates below.
This document authorizes no implementation, CLI usage, build, test, or release today.
The implementation plan is [here](../plans/2026-10-12-self-development-mvp.md).

## 1. Outcome and authority

Faber takes a goal for its own repository and produces a proven local change.
One lead plans. Focused workers return artifacts. Engine code controls execution and git.
The person sees the work, its cost, and decisions that need them.
After acceptance, the development instance can switch to its tested next build.
It then develops the rest of Full Chat. Full Chat is not a prerequisite.

| Source | Binding rule for this MVP |
|---|---|
| R96 | Bring this slice of Projects before Phase 2; do not bring all of Phase 3 forward |
| R82/R83 and this request | Grant git authority at project start; Seguro is this MVP's default |
| R86/R89 | Automatic work uses confirmed subscriptions; no automatic paid fallback or unknown billing |
| R88 | Pace each weekly account by 1/7 per day, approximately 14.28% |
| R90 | Own API-equivalent cost and shared account percentage are separate ledgers |
| R91 | Start every eligible ready node now, within cost, daily pace, RAM, and review capacity |
| R92 | Notify only when the person is needed; never take focus |
| R93 | Two working slots, proof and canary before promotion, last good retained, automatic rollback |
| R94/R95 | Public releases stay separate; development never touches `E:\Apps\Faber` |
| AGENTS.md | Original code; hidden processes; no credential access; locales; proof before coherent commits |

R83's research proposes Equilibrado as the general default. R96 and this request narrow it to Seguro here.
An explicit Equilibrado grant still permits the limited remote steps described below.
R91 supersedes the roadmap's estimate-only budget warning for this slice.
An estimate may warn. A live spending cap must stop admission before its boundary.
R93 does not permit overwriting R95's installed stable release.
The two slots belong to a separate local development instance.

## 2. Scope and completion

| Included | Deferred |
|---|---|
| Existing local git repository; one active mission per repository | Repository creation, Livre, target/main branch merge |
| Lead, short contracts, dependency graph, decisions with authors | General catalog, all 15 powers, taste learning |
| Worker grants, area leases, per-writer worktrees, artifacts | Swarms, peer chat, cloud workers, permanent managers |
| Read-only research/review/test proposals; one initial writer | Unqualified parallel writers and performance promises |
| Serial proof/review/fast-forward queue | Speculative merge trees, merge batches, public release automation |
| Seguro; optional push and draft PR under Equilibrado | PR ready state, public tags, deployments, paid acceleration |
| Minimal Projects board and local self-update | Full Chat work and broad software specialization |

Parallel writers are supported only after disjointness, containment, metering, and class qualification pass.
They are not needed to meet the first usable date. Studio v0 launches read-only work first.
In v0, test writing produces a proposed test file or patch artifact outside the source checkout.
Only the admitted writer or integrator applies it to the repository.

The MVP is complete when a real, small Faber goal lands on its own local branch through both vendors.
The running development instance then builds, canaries, promotes, and resumes with that exact accepted tree.
A forced early failure rolls back automatically. Stop, crash, and restart preserve unfinished work.
Read-only planning alone is useful progress, but does not satisfy R96.

## 3. What exists on 2026-10-12

These findings come from source reads. No runtime behavior was tested for this document.
Roadmap descriptions of legacy gaps are not proof of the current source state.

| Existing code | Reuse | Missing for this MVP |
|---|---|---|
| `engine/src/model.rs`: Project, Conversation, RunningItem | Project identity and private conversations | Repo identity, grant, mission/task/lease records |
| `engine/src/engine.rs`: `cwd_of`, `launch` | Session lifecycle, event emission, idle closure | Project cwd currently points at the user's checkout; no lead or worker coordinator |
| `engine/src/store.rs` | JSON, temporary write then rename | No mission journal; conversation/project/ledger write errors are currently discarded |
| `engine/src/usage.rs`, `claude.rs`, `codex.rs` | Cumulative cost settlement and cache counters; resumed thread deltas | Complete priced accounting, live cost events, invocation exposure bounds |
| `budget.rs`, `ledger.rs`, `signals.rs`, `engine/auto.rs` | Billing, quota windows, pace, Auto dispatch gate | Global daily/run/worker grants; the router multiplier alone is not a hard daily stop |
| `claude_args` | Official stream-json driver | Projects omit explicit permission mode; no writer scope enforcement |
| `codex_thread`, `thread_params` | `workspace-write`, read-only side requests | Existing Projects request escalation; native tool isolation needs a capability gate |
| `engine/proc.rs`, `cli.rs` | Shell-free hidden spawn; npm shim resolution | `stop` kills only the child; add graceful completion and tree containment |
| `src-tauri/src/main.rs` | Fixed command list, window destruction, installed updater | Local slots, headless canary entry, quiescence, independent rollback helper |
| `ui/App.tsx`, `Running.tsx`, `Ask.tsx`, `controls.tsx` | Shell, project selector, decisions, theme controls | Goal/board/cost view; safe project enrollment |
| `src-tauri/src/tray.rs`, `tray/decide.rs` | Needs-you deduplication and focused-window suppression | Fullscreen policy, original sound, two Settings switches |
| `testkit/`, `tests/fixtures/`, `tests/ui/` | Fake CLIs, hidden desktop, audited scenarios | Project, queue, recovery, containment, and local-update scenarios |

Do not repeat the already implemented per-turn settlement task from the quota plan.
Extend it and add regression coverage where Projects needs stronger guarantees.
There is no durable mission journal to reuse today. Add small JSON records, not a workflow database.

## 4. Project enrollment and git authority

The project picker validates an existing non-bare repository with a committed base.
Canonical repository identity uses the real common git directory, not the displayed folder name.
Linked worktrees of the same repository share one coordinator and queue.
Reject path aliases, junction escapes, malformed refs, and an already-owned repository lock.
A non-git folder gets an explanatory locale key. Never run `git init` implicitly.

At start show the repo, starting revision, git mode, author identity, and self-update trust.
Read identity through git's ordinary configuration command; never read provider credentials.
For Equilibrado, show the selected remote and the account name reported by `gh`.
Use the existing authentication of git/gh without exposing tokens or credential helper output.
Missing identity blocks commits with one setup card; it does not invent an author.
The person grants the selected mode once. No answer grants no effects; planning remains read-only.
Keep approval bound to repository identity and grant revision. Revocation takes effect before each effect.

| Mode | Allowed after the grant | Still blocked or parked |
|---|---|---|
| Seguro, selected by default | Separate worktrees; local commits; fast-forward the mission's `faber/...` branch | All remote effects and changes to the person's branch |
| Equilibrado | Seguro plus non-forced push of that exact own branch and one draft PR | Main/target merge, marking ready, repo creation, releases |
| Livre | Not offered | No hidden fallback to it |

The mission branch is `faber/<mission-id>`; each writer gets a detached private worktree.
The original checkout, index, branch, dirty files, ignored files, and untracked files stay intact.
The base is the selected committed revision. Do not quietly include unsaved owner edits.
If those edits affect the goal, park that dependency and explain how the person can include them.
Faber never resets, cleans, stashes, checks out over owner work, deletes refs, or rewrites published history.
Commit only explicitly accepted paths. Never stage the whole directory blindly.
Check outgoing content for secrets before every remote operation; a match blocks the operation.
Checks never inspect credential stores. Ref changes use compare-and-swap against the recorded head.
Remote divergence blocks; never resolve it with force-push or rebase of a published branch.
An unavailable git/gh or failed push leaves the proven local commit intact and the remote receipt pending.
Creation of a duplicate PR is prevented by reconciling the own branch and existing draft after uncertain delivery.

## 5. Small execution boundary

A worktree is not a sandbox. Deny rules and a prompt are not sufficient isolation on native Windows.
All git runs in Faber's engine as argv, never through a shell.
The official CLIs remain unmodified. They never receive git mutation authority.
Claude Code gets explicit permission mode and deny rules for git network, history, and destructive commands.
Codex writers get `workspace-write`; escalation is denied and shared git metadata stays unwritable.
Verify linked-worktree common metadata as well as the `.git` file.

The smallest MVP path uses structured file requests and patch proposals between CLI turns.
The engine reads scoped source, applies the proposed changes, and runs named proofs.
Workers still have one worktree each, but do not need arbitrary shell access to edit it.
This avoids a general command parser and a new MCP server in the first slice.
The full Phase 3 toolbox can expose the same engine operations later.

| Role | Allowed engine requests | Forbidden authority |
|---|---|---|
| Lead | Read scoped source; propose/amend graph; brief; request accepted proof IDs; ask decision; integrate request | Write source, execute arbitrary commands, grant money, choose its own reviewer |
| Research/test proposal worker | Read fixed source; return findings or proposed test artifact | Source writes, repo metadata, other workers, native subagents |
| Writer | Read leased scope; propose exact file changes; request named proof; return manifest | Push/commit/delete refs, other areas, altering the grant or mission state |
| Reviewer | Read composed candidate and connected code; return pass or blocking findings | Source mutation and reviewing its own work |
| Engine | Validate packets, manage scopes/worktrees, apply patches, run proofs, commit and promote | Unapproved external effects and protected destructive actions |

Use a strict versioned JSON envelope, one action per turn, with bounded paths and content sizes.
Actions are `read`, `plan`, `amend`, `patch`, `proof`, `decision`, and `complete`.
Supply source through the validated `read` response; support requesting missing callers without loading the whole repo.
Every envelope names task, attempt, base tree, contract digest, and lease generation.
Unknown actions or fields, stale identities, oversized content, and invalid paths fail closed.
Patch payloads are explicit file operations with expected content hashes, not executable text.
Both rename paths are checked. Source-file deletion is allowed only in a disposable leased candidate, with recoverable evidence.
Branch/repository deletion and deletion outside the candidate are always blocked.

Disable native command, write, subagent, hook, plugin, and unapproved MCP escape paths for these sessions.
Use supported official per-session controls; do not invent flags or edit the person's global CLI configuration.
Capability verification must prove there is no bypass through native tools, shell chains, aliases, or network tools.
If an official CLI cannot enforce that role policy, do not admit an automated writer on it.
Review/read-only operation also needs verified read and command restrictions.
This is a first-week feasibility gate, not a claim that the current drivers already support it.
An engine broker and a tested policy replace repeated human approvals for routine in-scope edits.
Do not extend the existing `allow_always` tool-name grant into broad shell authority.

Proofs are named, versioned executable/argv/cwd definitions, fixed before worker dispatch.
For Faber use the AGENTS.md proof set. npm scripts resolve to the real Node executable and script.
Faber does not launch `.cmd` through `cmd /c`. No arbitrary model-supplied shell line is accepted.
Repository scripts are executable code. Run candidate checks in a disposable containment environment.
It has no credential directories, provider secrets, remote git authority, visible console, or user desktop.
Dependencies must already be installed or restored under a separate authorized setup step.
No implicit network installation or interactive credential prompt is part of a proof.
The existing testkit proxy is not a firewall; extend containment and test outbound/write denial before relying on it.

## 6. Lead, contracts, and the graph

One live Projects session keeps the goal and answers the person.
Engine code owns scheduling and integration. No extra manager model does bookkeeping.
Before big work the lead returns a short plan, acceptance, affected callers, and a graph.
The start card shows the plan and limits once. It is not an approval after each routine step.
Technical choices are logged as AI decisions. Direction, taste, money, and legal choices belong to the person.

| Contract field | Minimum content |
|---|---|
| Identity | Owner, version, digest, base revision |
| Boundary | Paths, callers, public shape, states, errors |
| Examples | One success and one failure; compatible saved-data example |
| Proof | Named commands, expected outcome, integrated acceptance |
| Dependencies | Required accepted task IDs; decision IDs; fixed contract inputs |

The engine rejects cycles, missing nodes, overlapping write areas, and duplicate deliverables.
A task becomes ready only when its required artifacts are accepted and its contract is current.
Contract-ready UI work may use mock data while engine implementation runs.
Real end-to-end acceptance still depends on both implementations.
One owner changes shared schemas, manifests, exports, registries, tokens, and lockfiles.
Contract revisions invalidate affected artifacts and proofs, not the whole mission.

Example: contract C is accepted first. Engine task E, UI task U, and test-proposal task T depend on C.
E/U/T start together if eligible. Integrated proof I waits for E/U/T. Promotion P waits for I and review.
Studio v0 can overlap T and research with its one writer. It cannot call U a read-only job while U edits source.

R91 dispatch occurs after every completion, grant update, decision, or lease release.
No wave barrier or arbitrary agent-count target holds ready work.
Choose the longest remaining dependency path first, then value; use aging for ties.
Capacity blocks keep a reason key and next wake condition. One failed branch leaves independent work eligible.
Normal/autonomous mode is separate from git mode and can switch without restarting a mission.
Autonomous mode uses only harmless recorded defaults. It never defaults an approval or money/legal decision.
Dependent nodes park; other ready nodes continue. Self-update requires its separate trust grant.

## 7. Workers, leases, and return artifacts

Every writer attempt owns one worktree and one exclusive normalized area lease.
Case aliases, parent/child paths, symlinks, junctions, both rename paths, and generated output count as overlaps.
Read-only tasks use immutable source at a recorded revision. Their artifacts live outside source.
Do not give a live mutable checkout to a reviewer.
Mutable build caches, ports, and installs have separate ownership or run serially.

Lease fields: repository, mission, task, attempt, paths, base, contract digest, generation, process identity, state.
Acquire and persist before launch. A heartbeat timeout never frees a lease on its own.
Prove the old process tree dead before reassignment. Manual worktree edits quarantine that attempt.
The broker checks the current lease on every patch operation and verifies the final diff.
Workers cannot spawn workers or directly edit canonical mission records.

Briefs contain the goal, non-goals, scope, exact base, contract, relevant source paths, proof, grant, and stop condition.
Start with a fresh short context. Reuse the context for related repairs while useful and within its grant.
Keep model, effort, instructions, and tools fixed; an escalation starts a new bounded session.
Load AGENTS.md and relevant mandatory rules. Never compress away a security or proof requirement.
Use existing Auto. A role name is not a second router.

Return a result packet with diff/file artifacts, changed paths, exact revisions, executed commands, exit codes,
log references, checks not run, cost completeness, assumptions, risks, and the next action.
Use a short summary, about 300 tokens initially. Preserve full local evidence behind references.
A claim that tests passed is not a test receipt.
One initial attempt, one focused repair, and one bounded diagnostic escalation share the same accumulated caps.
No progress for two expected task durations triggers checkpoint and diagnosis, not endless polling.

## 8. Live cost, daily pace, and resources

Use disjoint token categories: uncached input U, cache read R, cache write W, output O.
Price each at its published model rate; separate cache-write lifetimes and reasoning where required.
Count native nested calls, lead, routing, summaries, retries, review, canary, and integration calls.
Claude cumulative totals settle against the saved session base. Reconcile complete model usage where available.
Codex cumulative thread deltas include resume and tool calls; price them locally.
Unknown spend stays reserved. An empty abort is not free work.
Pin a dated rate table to each daily allocation. Unknown model/rate closes admission.

| Ledger | Unit and use |
|---|---|
| Own cost | API-equivalent dollars, by turn/worker/mission/day; caps and actual cost |
| Account quota | Provider windows and percentage, shared with use outside Faber; account pace only |
| Exposure | Maximum admitted invocation cost not yet settled; reserves safe stopping room |

API-equivalent dollars are accounting, not a subscription bill or an API payment.
Never charge another person's account movement to a Faber worker.
For each weekly reset, daily periods start at `reset - weekly duration` and advance by one day.
Room in quota points is the minimum of today's unused 1/7 and the cumulative line at today's end minus weekly use.
Shorter and family windows reduce eligibility. Rounded readings require a resolution guard band.
Missing day-start evidence, stale readings, clock reversal, or an ambiguous reset closes unattended admission.
Reconcile the baseline on recovery; never infer an empty day or silently refill it after restart.
Unused previous days do not enlarge today's grant. A learned router discount cannot bypass this hard gate.
Outside use reduces remaining room immediately. Faber cannot guarantee what another account user will spend.

Daily own-cost allocation D comes from conservative measured capacity and fresh account headroom.
Keep non-overlapping, complete own-cost intervals with the same reset and enough percentage movement.
Use a lower observed cost-per-point bound; outside use makes this conservative, never evidence of own spend.
No trusted calibration means no invented dollar conversion or unattended mission grant.
Bounded useful read-only calibration needs a previously authorized own-cost ceiling and verified invocation bounds.
Otherwise park setup and report the missing evidence. Never run an uncapped probe to obtain a cap.

Allocate mission grants from unspent D after all existing Faber reservations.
Allocate worker, lead, and reviewer subgrants inside the mission grant; nested grants count once.
Reserve review, repair, and canary allowance before writer admission.
Estimates choose width only. They neither create nor increase a grant.
Replans, retries, vendor handovers, and new days retain historical spend and receipts.
Each new day's additional allowance is explicit and bounded; it does not reset a worker's lifetime cap.

The scheduler gates before every invocation and every tool continuation and on every usage update.
Admission reserves a verified input/output/nested-call maximum plus measured stop latency exposure.
If live fields are late, the entire enforceable invocation maximum must fit the remaining allowance.
A CLI budget flag or final-turn report alone is not an enforceable maximum.
If neither bounded invocations nor safe live stopping can be proved, that configuration is unavailable for capped automation.
Interrupt before usable room is exhausted. Save existing artifacts if a model-written handoff no longer fits.
Settle before returning unused grants. A breach disables that driver configuration pending repair.
No automatic paid credits. Unknown billing, paid fallback that cannot be prevented, or unknown exposure closes admission.
The earlier quota spec's no-reading first-turn exception does not enable unattended Projects here.

RAM policy starts at `max(0, min(4 GB, physical RAM / 4, available RAM - 2 GB))` for active work.
Reserve measured process-tree peaks plus 25%; include lead, reviewers, CLI/MCP children, and builds.
Unknown peaks require serial calibration. One heavy proof runs at a time initially.
Keep today's 250 MB idle test ceiling and the complete-app target under 500 MB.
No prewarm fleet; use existing idle close and release screen DOM when closed.
Initially at most two completed candidates wait for the queue. Reserve a reviewer before launching a writer.
Read-only and writer limits are safety qualifications, not a reason to postpone other eligible ready tasks.

Parallel writer classes require the Studio study's measured qualification before activation.
Start with ten class items across three representative requests and low prediction error/rework.
Use paired compact-serial/concurrent runs at equal acceptance; include failed spend and coordination.
Require no blocking defects, improved median/p90 time, and upper 95% mean token/cost ratio bounds at most 1.10.
The intended cost is the same item sum. The 10% comparison tolerance grants no additional spending.
Insufficient evidence leaves writers serial; it does not disable useful read-only fan-out.
Show measured time saved/extra tokens only with a comparable baseline; otherwise show unknown.

## 9. Serial merge queue

One coordinator serializes all integration for a common repository.
Writer proof is advisory; acceptance uses the exact composed candidate tree.

| Order | Engine action and receipt |
|---|---|
| 1. Receive | Validate artifacts, lease, grant, base, contract, paths, and secret scan |
| 2. Compose | Apply the patch to an isolated candidate based on the current `faber/...` head |
| 3. Prove | Run all mandatory project commands; retain argv, exit code, tree, toolchain, environment, logs |
| 4. Review | Other vendor reviews that exact composed tree, diff, contracts, callers, and proof in a fresh session |
| 5. Repair | One bounded repair owner; changed tree repeats proof and cross-model review |
| 6. Commit | Engine creates one coherent local commit with the configured author and required attribution |
| 7. Promote | Compare-and-swap/fast-forward only if branch head, tree, proof, review, and grant still match |
| 8. Remote, optional | Equilibrado only: secret scan, non-forced push of own branch, reconcile/open one draft PR |

Faber's proof set is lint, typecheck, UI build, format check, both clippy checks, and `cargo t`.
Use `cargo t --all` whenever UI or shell changes. Unknown reach uses the full set.
Check test selection on a clean candidate: pass its recorded changed paths or explicitly use `--all`.
Do not rely on clean `git status` selecting the new commit's tests.
Reuse a receipt only for identical tree, command, toolchain, dependencies, and environment.
Partial checks and mock contract success never substitute for required proofs.

A required other vendor that is absent, exhausted, or unaffordable leaves the task in review.
No same-vendor downgrade can promote this self-development MVP.
The broader roadmap's weaker one-vendor review remains outside this acceptance gate.
Reviewers cannot grant an exception for themselves or approve their own authored content.
Conflicts preserve both patches and assign one repair owner; no blind side selection.
Head movement invalidates the stale receipt. Recompose and repeat proof/review before landing.
Done means proven local landing. Remote failure is shown separately, not disguised as a successful draft PR.

## 10. Persistence, stop, and recovery

Plain project records live in `.faber/missions/<id>/`: plan, contracts, briefs, decisions, artifacts, receipts.
Private conversations and CLI session identifiers remain in Faber's existing data store.
Operational worktrees, builds, and private logs stay outside the source checkout, under development data.
Exclude operational records from outgoing commits using a local git exclude; never change tracked `.gitignore` silently.
No new telemetry or credential store.

Use schema versions, atomic replacement, and a single durable repository lock.
Record intent before each dispatch or git/update effect; record its receipt afterward.
All mission and safety-ledger write errors return keys and halt effects before state can diverge.
Corrupt safety state is quarantined and blocks dispatch; it cannot become an empty budget.
Only caches that grant no authority may fall back to empty data.

Stop means pause: close admission immediately, interrupt active sessions, checkpoint, and await a bounded grace period.
Then kill owned process trees if needed. Kill neither an unrelated process nor all threads sharing one server.
If one shared Codex server must stop, checkpoint all affected tasks and reconcile their unknown cost.
Do not free leases or reservations until termination and settlement are known.
Stop also cancels pending quota timers and local promotion. It never labels unfinished work done.
Resume rechecks the current grant, trees, contracts, billing, quota, and cost.

After crash/reboot, reconcile process identities, locks, worktrees, candidate heads, unsettled usage, and receipts first.
Never replay an uncertain command, push, commit, or promotion blindly.
Match commit intent by parent/tree/identity; match remote effect by ref/PR identity; ambiguous effects park.
Retries do not reset counters. Failed branches remain blocked while unrelated branches continue.

## 11. Local self-update loop

Use a separate development root under Faber's development data directory.
Canonical-path checks explicitly reject `E:\Apps\Faber` and installed release data as destinations.
Use two working slots, current and next, plus a retained immutable last-good build.
An atomic manifest names slot, commit/tree, build hash, compatible data schemas, and status.
Never overwrite a running executable. Never promote a build from dirty unaccepted source.
The public signed updater remains solely for the installed release channel.
Local release-mode binaries pass an explicit development channel and separate data root.
Do not infer installed status from `debug_assertions` alone.

| State | Required action |
|---|---|
| Accepted self-change | Recognize the enrolled Faber repository identity; enqueue the latest accepted tree |
| Prepare next | Snapshot exact tree; run proofs; build local executable/assets into next; record hashes |
| Canary | Run next headlessly on an isolated miniature development repository, through goal/worker/queue/review |
| Ready | Require green receipts, schema compatibility, canary, and self-update trust; retain current as last good |
| Quiescent | No window exists, no running turn/tool/proof/git transaction, all state flushed, processes settled |
| Swap | Independent local helper updates the manifest and restarts next without creating a window |
| Probation | New engine loads state, reconciles, writes health acknowledgment, and completes its first bounded operation |
| Healthy | Retain last good; resume saved work; next future change uses the inactive slot |
| Failed | Missing acknowledgment within 30 seconds, crash, bad schema, or failed first operation restores last good |

The helper is a small first-party binary in the shell package, launched only for a local transition.
It monitors next through a bounded five-minute probation and persists the pending transition before exit.
It runs independently of the process being replaced, so it can recover an early crash.
At startup it reconciles interrupted swaps before launching a slot. Roll back once and quarantine the bad build.
No restart loop, updater download, installer UI, terminal, or window creation occurs during promotion or rollback.
A user's request to open during quiescence cancels the swap; during swap it waits for health before showing the window.
This explicit user request is the only reason a transition opens a window.
Build/check processes also use the hidden runner and process-tree controls.

Canary: a frozen tiny Rust repo has one failing normalization test and a narrow file lease.
Next receives a goal to fix it, launches a writer, runs its named proof, gets the other vendor's review, and lands locally.
Assert source preservation, cap enforcement, receipts, stop/resume, and no windows or remote effects.
Fake-CLI canaries run in mandatory offline proofs. A bounded real-CLI canary proves the local development workflow before promotion.
Reserve its cost inside today's allowance. If it cannot run, keep the build ready and wait; do not waive it.
Do not recursively trigger another self-update from the miniature repo.
Disable irrelevant discovery/router pulses there; load only the frozen canary configuration.

Saved data must remain readable and writable by last good during probation and rollback.
Use additive compatible fields with round-trip preservation, or isolate new records in a versioned sidecar.
An old process must not discard new fields or recreate a consumed grant after rollback.
Destructive migrations are outside this MVP. Failed compatibility proof blocks promotion.
A change to the helper itself affects future transitions only; the current trusted helper remains in control until healthy.

## 12. Projects screen and notifications

Keep the existing titlebar, sidebar, topbar, Running column, tokens, type, and Curioso empty states.
This is the approved Projects board slice, not a new visual identity.

| Surface | Minimum behavior |
|---|---|
| Goal | Existing composer styling; goal, short plan, Start; text stays after failed enrollment/dispatch |
| Board | Ready / Running / Review / Done / Blocked; stable task rows with builder/reviewer and reason |
| Cost | Own cost / live cap; separate daily account pace and freshness; unknown values say unknown |
| Decisions | Reuse Ask/Running cards; author and default; never auto-approve an effect |
| Stop | Always reachable keyboard button while running; shows paused checkpoint and Resume |
| Task detail | Scope, diff/artifacts, command results, missing checks, risks; technical logs behind a click |
| Local build | Current/next/last-good status and why promotion waits; no completion toast |

The board uses the stage width, not the Chat reading column's 660 px limit.
Below available width, stack state groups into a scrollable list; no required horizontal drag operation.
Keep 720/900/1100 px shell breakpoints and 13–18 px text support.
Use words and shapes with status colors; amber remains only needs-you.
Do not style every quota wait or dependency as a human decision.
Use semantic lists/headings/buttons, visible focus, focus restoration, and restrained live-region announcements.
Automatic Running opening keeps focus and the existing typing delay. Reduced motion follows the current tokens.
All visible sentences, errors, reasons, shell strings, and sound/notification switches live in `ui/locales/`.
English is the source. Hand-author all existing locales under the project's language-review rule.

R92: focused Faber means no notification or sound; fullscreen/game means original sound only.
Normal background use means notification plus sound, each controlled by its own saved Settings switch.
If foreground/fullscreen detection is uncertain, suppress the visual notification.
Never focus or open Faber for a notification. Deduplicate by stable decision identity, including restart.
No notifications for completion, commits, proofs, automatic quota waits, or successful promotion.
Only a concrete choice, missing permission, or unresolved direction/money/legal blocker can produce needs-you.

## 13. Acceptance and delivery gates

| Gate | Required evidence |
|---|---|
| G1: authority | Dirty owner checkout preserved; no CLI git effects; deny bypasses/metadata escapes; revocation enforced |
| G2: spending | Inclusive cache/nested calls priced once; delayed/abort/resume cases; no cap breach under maximum exposure |
| G3: scheduling | Every eligible ready node starts without batch waits; all four brakes; overlapping writer rejection |
| G4: acceptance | Composed-tree proofs before other-vendor review; stale-head rejection; one commit and fast-forward receipt |
| G5: recovery | Stop/crash at every external-effect boundary; no duplicate effects, lost work, freed live leases, or reset spend |
| G6: self-update | Headless canary; exact-tree build; no-window swap; retained last good; forced early-failure rollback |
| G7: screen/resources | Locales, keyboard/focus, width/theme/motion checks; 250 MB idle; process leak/window audit |
| G8: real use | One bounded real Faber goal and local promotion using both vendors, within the current daily grant |

Dates are a planning target, not a safety exception.
Verify G1/G2 feasibility on 13–14 October before building the rest of the coordinator.
Build journal/git/queue and UI against a frozen contract on 15–18 October.
Complete local promotion and offline recovery on 18–20 October.
Use 20–22 October for the bounded real goal, canary, rollback drill, and small fixes.
Equilibrado and qualified parallel writers can follow the Seguro usable gate; they remain in the ordered plan.
If driver controls, calibration, or hidden containment fail, keep read-only planning available and mark R96 incomplete.
Do not promise the target date by substituting prompt instructions or retrospective cost reports for hard controls.

## 14. Proposed roadmap diff; do not apply in this task

This is a proposal for a later coherent documentation commit. Phase numbers stay stable.
It changes order and advances only the prerequisites this MVP needs.

```diff
--- a/docs/roadmap.md
+++ b/docs/roadmap.md
@@
-  software area is ready, Faber is built with Faber.
+  self-development MVP is ready, Faber builds the rest of Full Chat itself (R96).
@@
-  off. Faber never kills work because of the budget.
+  off. Faber preserves unfinished work. Projects narrows or pauses before its live worker, mission and daily cost caps;
+  an estimate overrun only warns. R88/R90/R91 govern admission, not the estimate.
@@
-3. Sending, publishing, paying or deleting outside a copy asks for approval every time. Changing Faber itself
+3. Sending, publishing, paying or deleting outside a copy asks for approval, except the exact own-branch push and
+   draft PR already granted by Equilibrado at project start (R82/R83). Git never permits unrelated publication.
+   Changing Faber itself
    (installing, updating or removing a tool, changing a setting) asks too, with "allow once" and "always allow this
@@
-5. Another model reviews the work. In code, the writer never reviews its own work. Creative work gets two judges from
+5. Another model reviews the work. In code, the writer never reviews its own work. Self-development promotion requires
+   the other vendor; unavailable review waits. Creative work gets two judges from
    different vendors when both are connected.
@@
+### Self-development before Full Chat (R96)
+
+A minimal Projects slice comes after the new base and before Phase 2. Target: 2026-10-20 to 22.
+It includes a lead, contracts and ready dispatch, Seguro by default, worker worktrees and leases,
+a serial proof/review/fast-forward queue, Studio v0 read-only fan-out, daily own-cost grants,
+a minimal board and decisions, needs-you notifications (R92), and local next-build promotion/rollback (R93).
+Equilibrado adds only own-branch push and draft PR. Parallel writers need disjoint areas and measured qualification.
+Catalog, the remaining powers, Livre, and public release automation are deferred.
+The local development instance never touches the installed release in E:\Apps\Faber (R95).
@@
 | 1 | New base | the FSL-1.1-ALv2 license and these documents, Faber's own Rust test runner, the design system and themes, the identity, the shell layout and running list, the tray number and notifications, the first run and tutorial, Settings, and the CLI download written straight to disk. No public release: it is checked in the live app and a local installer build |
+| 1a (R96) | Self-development MVP | the minimal Projects slice above; closes the project execution, handoff, stop and live-budget gaps needed to let Faber build Faber before Full Chat |
@@
-| 4 | Excellent software, ending when Faber builds Faber | nightly CLI version tests and the live manifest, one shared copy of each MCP server per project, and a RAM budget for parallel workers |
+| 4 | Excellent software | generalize the self-development workflow; nightly CLI version tests and the live manifest, shared project MCP infrastructure, and qualified department writers |
@@
 ## Updates
+
+Local self-development uses current/next slots and a retained last good. Exact-tree proofs and a small canary gate
+promotion. Swap only with no window and no active effect. An independent helper rolls back an early failure.
+Saved data stays compatible with last good. This channel is separate from signed public updates (R93–R95).
@@
-**Progress view.** Each part of the work is one row that moves left to right through four columns: Waiting, Doing,
-Reviewing and Done.
+**Progress view.** The self-development MVP uses Ready, Running, Review, Done and Blocked. Ready tasks can wait only
+for an explicit resource brake; Blocked tasks name their missing dependency or decision. The remaining Projects
+detail view follows the later full implementation.
```

Also revise the CLI legacy-gap table when implementation is proven, not when this proposal is written.
Mark Projects' explicit permissions/model/effort, written handoffs, process-tree stop, capabilities/reason keys,
and live running budgets as prerequisites closed in 1a. Leave remaining Chat gaps assigned to Phase 2.
Keep full catalog/powers in Phase 3 and public-release approval in Phase 5.
Link this spec from the Phase 1a row and the Updates paragraph when that later diff is applied.

## 15. Sources and unresolved technical gates

Read-only basis: `AGENTS.md`, `docs/roadmap.md`, `docs/design.md`, the Auto quota pacing spec,
current engine/session/store/usage/budget source, shell/updater/tray source, and testkit entry points.
Owner basis: `faber-notas/decisoes.md` R82/R83/R86/R88–R96, `pesquisa/git-automacao.md`, and `pesquisa/studio-design.md`.
The Studio rollout predates R96's reordered slice; its safety and measurement gates remain binding.
No external capability, price, performance, or provider-policy claim was newly verified for this document.

| Technical uncertainty | Resolution and fallback |
|---|---|
| Supported CLI tool restrictions and hidden nested calls | Capability/escape tests first; refuse unsafe role configurations |
| Enforceable live/invocation maximum | Bound and measure before dispatch; no unattended automation without it |
| Daily calibration and shared-account readings | Conservative evidence or saved waiting state; no guessed unlimited grant |
| Candidate script containment | Hidden disposable environment with write/network boundaries, tested escapes |
| Headless restart and compatible old writes | Independent helper plus round-trip/recovery tests; keep current if invalid |
| Parallel writer benefit | Qualification after v0; no public same-cost/speed claim from fixtures |

These are engineering work, not owner taste or money decisions. No new owner decision is required to write this plan.

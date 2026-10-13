# Self-development MVP implementation plan

Date: 2026-10-12. Status: R105 order; T1 done, T2/T4 done pending integration and their remaining proof gates.
Spec: [Self-development MVP](../specs/2026-10-12-self-development-mvp-design.md).
Goal: Faber completes a proven local change and promotes its next development build.
22 October is an internal milestone; acceptance is conditional on the complete bar and safety gates.
R96 moves this slice ahead of Full Chat. The rest of Chat, catalog/installs, route learning, Livre, and public release automation wait.

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

R105 adopts the [rework fit analysis](../../../../faber-notas/pesquisa/rework-fit.md), Proposed final order.
Keep task IDs. T1 is done; T2 (driver policy) and T4 (git broker/leases) are done pending integration,
second review/CI and G1/G2 qualification. Their accepted journal and safety contracts remain binding.

| Order | Task | Dependency / owned area |
|---|---|---|
| 1 | T3: Mission grants and admission | T1 + accepted Auto pacing; shared spending adapter and spend-path red tests |
| 2 | T5K: On-the-fly kits | T1/T2 gate; one owner freezes additive wire/kit/bar contracts, inventory, delivery, attestation, gaps |
| 3 | T5B: Complete software bar | T5K contract; compiler, grounded checks, native proofs and receipts |
| 4 | T5: Recoverable runner and queue | T2/T3/T4/T5K/T5B; persona, verbatim envelopes, reflexes, scars, pins/escalation |
| 5 | T9 + T10: Projects and needs-you | T5; minimal UI, receipts/assumptions, AS-1..4 and existing notifier |
| 6 | T11: Local build/canary/rollback | T5/T9; compatible data, owned slots/disk and immutable last good |
| 7 | T13: Acceptance | T3/T5/T9/T10/T11; software parity, bounded plain-CLI probes, G1-G10 and AS-1..9 |

Run G1/G2 feasibility before automated roles. T9 mocks/ux-polish reconciliation and T11 pure slot state may
start now; disjoint T5K/T5B modules may overlap after the shared schema freeze. Serialize integration and heavy proofs.
Ready nodes start within grants, daily pace, RAM and review capacity. Shipped missions keep one source writer.
Reuse phase-1-base, notify-sound and ux-polish; do not rebuild them. T14/T15 remain Later.

Task 2 implementation and both reviews are integrated. Role containment and exposure qualification remain blocked;
see [policy probes](../../research/2026-10-12-project-policy-probes.md) and
[second-review disposition](../../research/2026-10-12-task-2-opus-review.md). Existing manual launch defaults remain intact.

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

## Task 5K: On-the-fly kits and the shared contract

Sources: [tool selection](../specs/2026-10-12-tool-selection-rework-design.md), MVP G9,
and the fit analysis. One owner revises wire/kit/bar contracts additively; retain old durable records.
Implement inventory/cards, scoped selection, frozen pins/hashes, inline delivery, attestation, gaps and kit receipts.
Native levers need capability/escape proof; unsupported isolation blocks admission.
Test first: incompatible loadouts, hash drift, unwanted discovery, required gaps, crash/reload receipts and selector targets.
Run GA and independent review; record G9 measurements without paid probes or performance claims from fixtures.

## Task 5B: Complete software bar and compiler

Source: [generated pipelines](../specs/2026-10-12-generated-pipelines-design.md), MVP G10 and complete software bar.
Persist the deliverable/risk sheet, grounded additions, check owners, scoped paths and compile/check receipts.
Back-chain and prune; cheap proofs precede exact-tree review. Native advisory/license/registry/secret checks use fixtures.
Test first: orphan/generic nodes, unsupported grounds, missing mandatory checks, stale receipts and partial rerun invalidation.
Keep every mandatory bar check; conditional checks need evidenced applicability. Run GA and independent review.
T13 supplies real parity/benchmark evidence; fixture success never substitutes for live required proofs.

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
- [ ] Record software/map parity, bounded plain-CLI probes, G1-G10 and AS-1..9 only with evidence; missing controls keep R96 incomplete.
- [ ] Run GA, cross-vendor review, commit `test: prove the self-development MVP`.

## Task 14 — Equilibrado remote: push, draft PR, CI status, merge card

R108 slice 1, moved out of "Later". Equilibrado only; Seguro keeps denying every remote effect, reads included.
Rules come from the design (git modes, queue step 8, section 10): exact own-branch non-forced push, one draft PR, secret
scan first, intent then receipt, a third "maybe done" state settled by reading live remote state, never a retry.
R108 adds reading the PR's check results and a merge card. Merging needs the owner's explicit yes, given for one exact
card: the repository, the PR number, the full head SHA and the base branch. A yes for any other card, or the same card
with another head or base, does not count, and the card is checked again against the live PR right before the merge.

Files (all new, so Task 5 does not conflict): `engine/src/project/remote.rs` (one `pub mod remote;` line appended at the
end of `project.rs`, where Task 5 does not edit), `testkit/src/remote.rs` (stateful fake git + gh, `pub mod` line in
`testkit/src/lib.rs`), `testkit/src/remote/tests.rs` (unit tests of the testkit library: an integration test there would make `cargo test`
rebuild the running `faber-test` binary), the `reason.remote.*` keys in the five `ui/locales/*.ts`.

Design (as hardened by two independent security reviews; a worker can write the repository's config, refs and files, so
nothing read from them is trusted):
- `Runner` runs one `Call { tool: Git | Gh, cwd, args }` and returns `Reply { code, stdout }` or `Lost::Missing`
  (not installed, nothing ran) or `Lost::Interrupted` (started, no answer, or killed). `Process` is the real one:
  shell-free, the person's own environment, git credential helper and gh login used as they are, hidden window, no
  prompts, repository-redirecting variables removed, a 180 s timeout that ends the whole process tree (a Windows job
  object), and output capped at 16 MiB: more than that is a failure, never a partial success. Faber reads no token and
  no credential file; stderr is never read.
- `Ledger` is the seam to the mission journal: current grant, `intent`, `receipt`, `settle` (replaces `Unknown` with
  an engine-read Done or Failed), `open` (remote intents without a settled receipt).
- `Target { cwd, identity, revision, repo (owner/name), remote, url, branch (faber/...), head (full oid), base, attempt }`.
  Everything is validated before any process runs. The pushed ref is `<head oid>:refs/heads/<branch>`, so only the proven
  commit leaves, never a branch that moved since.
- Reads of the remote never run inside the repository: `ls-remote`, `gh pr list/view/create/merge` and the gh login run
  in the runner's neutral folder (`Runner::neutral()`) against the granted URL and `--repo` itself, so the repository's
  own config cannot redirect them. That folder is Faber's own, created by the shell next to its other data, never the
  shared temp folder: `Process` refuses to run a neutral call if the folder or any parent holds a `.git`. Only the push
  needs the checkout, and it goes to the granted URL, not to the remote name.
- Config guard before the push, an allowlist (a blocklist of bad keys was missed three times): `git config --list --show-scope
  --name-only`. In the local and worktree scopes only these keys may appear: the `core.*` basics `git init` and the
  platforms write (`repositoryformatversion`, `filemode`, `bare`, `logallrefupdates`, `symlinks`, `ignorecase`,
  `precomposeunicode`, `longpaths`, `autocrlf`), `extensions.worktreeConfig` and `extensions.objectFormat` (the layout git
  must understand to open the repository), `remote.<granted remote>.url` and `.fetch`, `branch.<own branch>.remote`,
  `.merge` and `.vscode-merge-base`, and `user.name` and `user.email`. Section and variable names are compared without
  case, a subsection exactly, as git does: `[remote "Origin"]` is another remote than `origin`, so any other remote or
  branch section is refused. Anything else blocks the push, so does an unparsable line: that includes every `include`
  and `includeIf` (`--list` follows them and judges what they bring, and the include key itself is not listed), `gpg.*`,
  `credential.*`, `url.*`, `core.hooksPath`, `core.worktree`, `remote.*.pushurl` and the promisor keys. The command scope
  may hold only Faber's own pins: the environment's `GIT_CONFIG_PARAMETERS`, `GIT_CONFIG_COUNT`, `GIT_CONFIG_KEY_<n>`,
  `GIT_CONFIG_VALUE_<n>` and `GIT_CONFIG` are removed from every call, the network reads in the neutral folder included.
  `GIT_CONFIG_GLOBAL` and `GIT_CONFIG_SYSTEM` are left alone on purpose: they are the person's own way to place the files
  the global and system scopes already trust, and only the person can set them for Faber's process. The remote name
  must also resolve to exactly the granted URL (`remote get-url --push --all`, one line, equal).
  Repository layout: `info/grafts`, `shallow`, `objects/info/alternates` and `objects/info/http-alternates` change what
  `git log` walks or where objects are read from, and `--no-replace-objects` does not undo grafts. One
  `rev-parse --path-format=absolute --git-path ...` asks git where it would read each (a linked worktree resolves to the
  common dir), and the push is refused (`reason.remote.layout`) if any exists, even as a dangling link.
  `GIT_GRAFT_FILE` and `GIT_SHALLOW_FILE` are removed from the environment so those default places are the ones read.
  Protocols: `protocol.allow=never` is pinned on every git call, and `git_net` turns on the one transport the granted URL
  needs (`protocol.https.allow=always`, or `ssh`) for `ls-remote` and the push. `GIT_ALLOW_PROTOCOL`, which beats every
  `protocol.*.allow`, is set in the child's environment to the same single transport (`none` for a call with no URL, so
  `file://` and `ext::` are refused even where the environment allows them; gh gets `https:ssh`).
  The pins stay as the second layer: hooks off (`core.hooksPath` to the null device, and the push uses `--no-verify`),
  `log.showSignature=false` with `--no-show-signature`, `safe.bareRepository=explicit`, and `GIT_NO_LAZY_FETCH=1` so a
  partial clone's promisor remote is never contacted by a scan. The neutral folder is used as its real path; it is refused
  if it is a link, has a bare repository's layout, or the real path or any parent holds a `.git`.
  A hidden scenario (`remote git` in `testkit/src/screen/remote_git.rs`) runs these same calls with real git, in an
  environment that allows every transport and injects config of its own: a forged `gpgsig` commit with `log.showSignature`
  and `gpg.program`, includes, worktree config, other and differently cased remotes, hooks in `.git/hooks` and at
  `core.hooksPath`, an `ext::` push and a `file://` read, injected `GIT_CONFIG_*`, grafts (also from a linked worktree),
  a shallow clone, a shared clone and `http-alternates`, a partial clone, a bare layout and a junction as the neutral
  folder. Each case has a control that proves the attack works without the guard on that git.
- `preflight(url)`: `gh auth status`, the account login, and `gh repo view <url>`, all outside the checkout, give
  `Account { login, repo }` for the start card. gh must act as the account the grant names (`remote_account`); a grant
  without one acts as none (`reason.grant.noAccount`).
- `push`: grant, no open remote intent, config guard, URL pin, live `ls-remote` of heads and tags, secret scan of
  everything that would leave, ancestor check (diverged blocks), intent, grant re-check, push without force or hooks.
  The scan excludes the commits the live remote advertises (never local tracking refs, which are forgeable) and reads
  every blob as text (`--text`), merge commits against every parent (`--diff-merges=separate`), commit messages, and a
  copy with zero bytes removed (UTF-16). Every option that changes what git prints (`log.showRoot`, `log.diffMerges`,
  `i18n.logOutputEncoding`, `--encoding`, colour, replacement refs) is set on the command line, which beats the
  repository's config. Any scan failure, or output over the cap, blocks the push.
- A call that gave no clean answer: an exit the tool reported is settled at once by reading the live remote. A call that
  was killed or timed out is recorded `Unknown` at once and not read at all: a helper it started may still land.
  `Lost::Killed` means the whole tree is proven gone (Windows job object set up); `Lost::Interrupted` means it is not
  (no confinement, any other platform, a reader that never ended). The receipt records `tree = gone | unproven`.
  `reconcile` settles by reading, and nothing is ever run again: finding the effect is `Done` in every case, but
  finding nothing is `Failed` only when the tree is `gone`; otherwise it stays `Unknown` for a later read or the
  person. An intent with no receipt (a crash) has no such proof either.
- `open_pr`: only PRs of this repository (`!isCrossRepository`) whose branch is the own branch count; a fork's PR of the
  same name is ignored. An existing PR of the own branch in any state is reused (refused if aimed at another base).
  Otherwise verify the remote head, intent, `gh pr create --draft`, then always read the list back and accept only a PR of
  the proven head and base, so `Done` carries the PR number.
- `ci`: `gh pr view --json statusCheckRollup,...` into `Ci { Pending | Passing | Failing | None, failed names }`; a PR
  that is not ours is `reason.remote.foreign`, and a head that is not the proven head is `reason.remote.headMoved`.
- `merge_card` builds `MergeCard { repo, pr, head, base, ci }` and the decision id `merge-<pr>-<sha256 of the whole
  binding>`, so two heads that share a prefix are two cards. `OwnerYes` can only be made from a Decision that a Person
  answered "yes", under that id, carrying that exact binding. `merge` refuses without it, in Seguro, when the card is not
  the target's repository, head and base, when the live PR moved (head, base, closed, fork), when checks are pending or
  failing, or when gh acts as another account; it runs `gh pr ready` if needed and `gh pr merge --merge
  --match-head-commit <head>`, never deleting the branch, with the same intent/receipt/settle rule.
- `reconcile` after a crash reads the live state of every open remote intent and settles it, under the same grant; it
  never re-runs one.

Tests first (`testkit/src/remote/tests.rs`, scripted fake, no process, no network). Every safety guard has a test that
fails when the guard is removed: the pull request lists the mutations that were run to prove it. Covered: Seguro denies
every effect and read; a revocation between intent and effect stops it; gh missing and logged out return keys with no
intent; the push goes to the URL with the exact oid; already-pushed sends nothing; divergence blocks; the scan reads
binaries, merges, messages and zero-spread text, excludes the live heads only, and fails closed; repository config in
the local, worktree and command scopes blocks; extra push URLs block; malformed targets never reach a process; reads
never run in the repository; lost-after, lost-before, killed and unreadable deliveries for push, create and merge;
fork, wrong-base and wrong-account PRs; CI states; the merge card binding by full head, base, number and repository;
merge refused without a person's yes, in every mismatch, and with failing CI; no force, admin or branch deletion in any
recorded call; `Process` builds shell-free argv with the expected environment (no spawn).

Focused proof: `cargo test -p faber-testkit remote::tests`, then the full set.

Seam for the runner (after Task 5 merges, because it edits the runner and the journal):
1. After the local fast-forward and proof, call `remote::publish(&mut Process::find()?, &mut ledger, &target, &pr)`
   (`preflight`, `push`, `open_pr`), show the result as a separate remote receipt, never as "Done".
2. Implement `Ledger` for the mission journal: `intent`/`receipt` over `record_intent`/`record_receipt`, `settle` over a new
   journal method that lets engine-read evidence replace `Unknown` (today only a Person decision can, in `Journal::reconcile`),
   `open` over `pending()` filtered to Push/DraftPr/Merge.
3. Add `IntentKind::Merge` and `Effect::Merge` (`Step::Merge` and the PR reads map to `DraftPr` until then) and a merge
   card in the board (`MergeCard::decision()`, question key `decision.remote.merge`); the Person's answer on
   `card.decision_id()` is turned into `OwnerYes::from_decision`. The runner builds the card from the live PR and
   passes the same `Target` (full head and base) to `merge`, which checks them against the card.
4. At recovery call `remote::reconcile` for open remote intents before dispatching anything.
5. Create the neutral folder (for example `<faber data>/remote-cwd`) and build the runner with
   `Process::find(neutral)`.
6. Show the account from `preflight(url)` and the `repo` on the Equilibrado start card, and store the account in the
   grant (`remote_account`): without it every remote effect is refused.

- [ ] Write the failing tests and the fake, see them fail for the right reason.
- [ ] Implement `remote.rs`, locale keys (en source, four by hand), see them pass.
- [ ] Run the proof set and `cargo t --all`, commit `feat: add the Equilibrado remote module`.
- [ ] Push `selfdev-remote`, open a draft PR into `selfdev`, wait for CI.

## Later — outside the MVP

- Former Task 15: qualified parallel writers. Disjoint leases plus measured Studio containment/cost/RAM/review gates;
  paired equal-acceptance evidence includes failed spend and coordination. Insufficient evidence keeps writers serial;
  10% comparison tolerance grants no extra spending.

## Calendar and stop condition

R105 estimates 230-364 remaining agent-hours. Acceptance is conditional, approximately late October to mid November;
22 October is an internal progress milestone. Follow the task order above; do not cut the bar to meet a date.
Do not add catalog/installs, a generic tool server, route learning, TL-ADE head-to-head or public release automation.
If spending or containment cannot be proved, stop automated execution and retain read-only planning.
If proofs or canary fail, retain current and unfinished work. The final report distinguishes source tests,
hidden integration proof, real-CLI evidence and checks not run.

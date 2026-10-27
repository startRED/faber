# T3 Code: source control, checkpoints and worktrees, study for Faber

Read-only study. Nothing was run, built or installed in the T3 clone. All paths are relative to
`t3code/`.

License: T3 Code is MIT (`LICENSE`, "Copyright (c) 2026 T3 Tools Inc."; `apps/server/package.json` says `"license": "MIT"`).
Any code ported from it must keep that copyright and license notice. Faber's own rules forbid copying code from
TL-ADE, TL-Control or TL-Orchestrator; they say nothing about MIT code, but the default here is still to write our own
code and reuse only the ideas, unless a small pure function is worth carrying over with its notice.

## 1. How T3 is put together (the parts that matter for Faber)

| Layer | Where | What it does |
|---|---|---|
| Per-turn checkpoints | `apps/server/src/orchestration-v2/CheckpointService.ts` | Captures the workspace before and after each turn, stores each capture as a private git ref, diffs turns, restores a turn. |
| Git backend for checkpoints | `apps/server/src/vcs/GitVcsDriver.ts` 814-1381 | Builds each snapshot in a temporary index. Restore, diff and authored-path listing live here. |
| Facade | `apps/server/src/checkpointing/CheckpointStore.ts` (157 lines) | Resolves the VCS driver per workspace path through `VcsDriverRegistry`. Keeps the store free of git details. |
| Pure diff parsing | `apps/server/src/checkpointing/Diffs.ts` (46 lines) | `parseTurnDiffFilesFromNumstat` and `isGitImport`. No I/O. |
| Worktree create / remove | `apps/server/src/vcs/GitVcsDriverCore.ts` 3676-3800 (create), ~4160-4231 (remove, prune) | `git worktree add` with a path rule, submodule handling, progress parsing. |
| Windows-safe removal | `apps/server/src/vcs/worktreeRemoval.ts` (156 lines) | Removes a linked worktree on Windows without following NTFS junctions. |
| Thread launch | `apps/server/src/orchestration-v2/ThreadLaunchService.ts` 254-284 | Checks that the base ref exists before a worktree is created. |
| Background setup | `scripts/setup-worktree.ts` (116 lines) | Runs dependency install in the new worktree in the background, with marker files. |
| Commit / push / PR in one action | `apps/server/src/git/GitManager.ts` `runStackedAction` 3018-3110+ | Chooses phases from the action (branch, commit, push, pr), generates the message with a model, then runs them in order. |
| Git workflow facade | `apps/server/src/git/GitWorkflowService.ts` (502 lines), outline 302-502 | Thin service over the driver and the manager: status, refs, `createWorktree`, `removeWorktree`, `pruneWorktrees`. |
| Shared hosting abstraction | `packages/source-control-core/src/server/SourceControlProvider.ts` (215 lines) | One Effect service with optional hooks per host: `listChangeRequests`, `getChangeRequest`, `createChangeRequest`, `checkoutChangeRequest`, `getRepositoryCloneUrls`, `createRepository`, `getDefaultBranch`, plus link and identity helpers. |
| Host implementations | `packages/source-control-github`, `-gitlab`, `-bitbucket`, `-azure-devops`, `-forgejo`, `-gitcafe` | Each host implements the same provider. GitHub is the largest (`GitHubPullRequestProvider.ts`, `githubStackActions.ts`, `githubStackRebase.ts`). Only the file list of the others was read. |
| Storage cleanup | `apps/server/src/storageCleanup.ts` 85-90, 119-125, 219-240, 416-470, 528-560 | Rule-driven cleanup of T3-managed worktrees, plus a report of folders nobody uses. |
| User docs | `docs/user/source-control.md`, `docs/user/project-settings.md` 171-208 | What the user sees: turn-change review, worktree location, cleanup rules. |

## 2. Checkpoints: one private ref per turn, built from a temporary index

**What T3 does.** Before a run starts, `CheckpointService.captureBaseline` (`CheckpointService.ts` 265-305)
records a snapshot. It is idempotent: `hasCheckpointRef` makes a second call a no-op. After each turn, `capture`
(361-516) takes another snapshot and diffs it against the previous one with `git diff --numstat -z`. Each snapshot
lives under `refs/t3/orchestration-v2/checkpoints/<scopeKey>/ordinal/<n>` (`checkpointRefForScopeOrdinal`, 161-172),
where `scopeKey` is the first 32 hex characters of SHA-256 of the scope id. Every capture runs under
`withWorkspaceLock`, so two captures on one workspace cannot interleave.

The snapshot itself (`GitVcsDriver.captureCheckpoint`, 818-1154):

- Uses a temporary index file at `<gitCommonDir>/t3-checkpoint-index-<uuid>` through `GIT_INDEX_FILE`. The user's real
  index is never written. It is deleted in `Effect.ensuring` (1153).
- Starts from a copy of the real index and fixes a racy-timestamp problem (868-888).
- Handles sparse checkout (850-970). Uses `git add -A`, or `-u` for workspaces that ignore files (1030-1047).
- Detects nested repositories with uncommitted work and recovers them (1048-1103).
- Runs `write-tree`, then `commit-tree -p HEAD -m "t3 checkpoint ref=<ref>"` with a fixed T3 author (1105-1146),
  then `update-ref` (1148-1152). The checkpoint is therefore an ordinary commit that holds the full tree and never
  appears on a branch.

**Why it helps Faber.** Faber's users are often non-programmers. They need "undo what the assistant just did" and
"show me what it changed in this turn" without learning git. This design gives both without touching the user's
branch, index or staged changes. Faber's worktree "copies" already give isolation per task; per-turn refs would add
history inside each copy.

**Caveat.** Each checkpoint is a full-tree commit. Disk cost grows with the number of turns and with the size of the
tree, so pruning matters (see section 5).

**Effort: M.** Idea to adopt; the ref layout and temp-index technique are not copyrightable in the way code is, but the
implementation should be Faber's own.

## 3. Turn diffs that show only the turn's own work

**What T3 does.** A turn's diff is built from numstat between the previous and the current checkpoint. Files that came
in through `git pull`, merge or rebase are dropped. `listAuthoredPaths` (`GitVcsDriver.ts` 1320-1365) returns `null`
when HEAD did not move; otherwise it returns the union of the paths changed against the parent in both refs, plus the
paths from `git log --diff-merges=remerge` in the range (1349-1364). `CheckpointService.capture` then keeps only
"authored" paths. `docs/user/source-control.md` lines 8-11 state the rule for users: a file stays in the list when the
turn edited it, committed it, or fixed a conflict in it. Restore still returns the full saved workspace.

`parseTurnDiffFilesFromNumstat` (`checkpointing/Diffs.ts`, NUL-split, rename-aware, sorted) and `isGitImport`
(lines 37-46) are pure functions over text. They could be ported almost unchanged to any language.

**Why it helps Faber.** Faber's "what changed in this turn" panel is only honest if an agent's `git pull` does not make
dozens of unrelated files look like the agent's work. This is the rule that makes the panel trustworthy.

**Effort: S** (pure functions) plus the authored-path rule (M, because it needs the ref history).
**Type: portable pure function for the parsers; idea for the authored-path rule.**

## 4. Restore a turn: destructive, and T3 says so by design

**What T3 does.** `CheckpointService.restore` (518-553) only restores a checkpoint with status `ready`, with
`fallbackToHead: false`. `GitVcsDriver.restoreCheckpoint` (1161-1247) runs `git restore --source <oid> --worktree --staged -- .`
(1181-1194), then `git clean -fd` (1210-1215), then recreates the directory and runs `git reset --quiet -- .` (1237-1244).
This makes the workspace match the checkpoint exactly, including deleting files the turn created.

**Why it matters for Faber.** This is the one place where a bug destroys a user's work. It is useful (undo a turn) but
it must never run without a confirmation and a safety checkpoint of the current state first. T3 gives the pattern: a
status enum that gates restore (`ready` only), and a checkpoint of the present state before the rewrite. Faber should
copy the gating idea, not the `clean -fd`, unless the user has confirmed it.

**Effort: M.** Idea only. Safety rule: restore must first capture the current state as its own checkpoint, and show the
user the list of files it will delete.

## 5. Worktree creation, background setup and cleanup

**Creation** (`GitVcsDriverCore.ts` 3676-3735):

- Path rule: `<worktrees dir>/<repo name>/<branch with / replaced by ->` (3682-3701). A custom location must be an
  absolute folder and not a drive root; the error text points the user to Settings.
- `git worktree add -b <new> -- <path> <base>` with `-c checkout.workers=<n>` read from the repo's config (3703-3713),
  and `GIT_PROGRESS_DELAY=0` so checkout progress reaches a pipe (3719-3721). Progress lines are parsed and shown.
- Submodules: `.gitmodules` decides whether to run `submodule update --init` (recursive or not). The mode resolves from
  a per-project file `t3.json`, then settings, then a default (3737-3775). A broken `t3.json` logs a warning and falls back.
- Before creation, `ThreadLaunchService.checkWorktreeBase` (254-284) checks that the base ref exists locally, as a remote
  tracking ref, or as HEAD, and fails with a typed error if none does. This stops a worktree from being half-created.

**Background setup** (`scripts/setup-worktree.ts`): runs `vp i --frozen-lockfile -- --yes` in the new worktree in the
background. Progress and result are written as marker files in `<worktree>/.t3/` (`setup-running`, `setup-done`,
`setup-failed`). Signal handlers (34-42) cancel it cleanly. A warning (78-100) explains that pnpm's store on another
disk breaks hardlinks and suggests `pnpm config set store-dir <parent>/.pnpm-store --global`. Env files are symlinked
from the project root (102-116), and only existing symlinks are replaced, so a real file is never overwritten.

**Windows removal** (`worktreeRemoval.ts` 17-156): Git for Windows before 2.54 follows NTFS junctions, and newer Git
leaves them behind. T3 therefore checks Git's own refusals first (lock file, submodules, modified or untracked files
unless forced), then deletes the folder with Node's `rm`, which unlinks junctions without following them, and only then
runs `git worktree remove --force --force`. If the delete fails partway, it rewrites the `.git` file so a later retry
still sees a worktree (147-153). `GitVcsDriverCore.ts` 4180-4197 logs leftover files that appear after Git has removed
the worktree.

**Cleanup policy** (`docs/user/project-settings.md` 178-208; `storageCleanup.ts`): per-project rules, inheriting from the
machine: remove after N inactive days, after a thread settles, after a merged PR whose commits are on the default branch
and whose head still matches (`storageCleanup.ts` 181-193), when a thread is deleted. "Keep worktrees with local
changes" has two levels: `tracked-changes` or `any-local-files`, which also counts ignored files, excluding
`node_modules` (`storageCleanup.ts` 528-560). Active sessions and shared worktrees always block removal. Folders in
the worktree directory that no thread uses are listed with their size but never removed (docs line 208).

**Why it helps Faber.** Faber's copies can each grow to several gigabytes (dependency folders, build output; see the
C: disk note in memory). T3 already has the policy shape Faber needs: explicit rules, a "never delete if it has local
changes" default, and a report of orphans with sizes. Its Windows junction handling is also a real hazard Faber will hit
with `rm -rf` on worktrees.

**Effort: M** for the policy and report; **S** for the setup-marker pattern.
**Type: idea for the policy; portable pure function for the path rule and the `.git` restore-on-failure step.**

## 6. One action for branch, commit, push and PR

**What T3 does.** `GitManager.runStackedAction` (`GitManager.ts` 3018-3110, continuing below) takes one action
(`commit`, `commit_push`, `commit_push_pr`, `create_pr`, `push`) and derives the phases from it: optional branch,
commit, push, pr (3027-3032). It emits a progress event with the phase list at the start, then one event per phase.
A detached HEAD fails with a clear message before anything runs (39-52). Commit and PR texts come from a text-generation
model chosen in project settings, with a separate "source control writer" model override (58-74). Branch names are made
by `formatGeneratedBranchName` and `sanitizeBranchFragment` (`packages/shared/src/git.ts` 20-37): lowercased, quotes
stripped, runs of unsafe characters collapsed to `-`, a 64-character cap, and a fallback of `update`.

**Why it helps Faber.** Non-programmers should press one button that says "save this and send it for review". The
phase list is what lets Faber show "committing... pushing... opening review" in plain words, and a human-readable
failure per phase. The branch-name sanitizer is a clean, safe pure function.

**Effort: S** for the sanitizer, **M** for the phased action with progress.
**Type: portable pure function (`sanitizeBranchFragment`); idea for the phased action.**

## 7. Agent credit removal from merge messages

`packages/source-control-core/src/server/mergeMessage.ts` (1-53) removes lines that name an agent as co-author
(`Co-authored-by:` with known agent emails) or a `Generated with ...` footer, and leaves fenced code and indented code
alone. Human co-authors stay. It is off by default and per project (`docs/user/source-control.md` 173-177).

**Why it helps Faber.** Faber's users run Claude Code and Codex, which add their own co-author lines. A user who is
not a programmer will see those lines in history and may think they are noise or a mistake. A setting to hide them in
the Faber UI (not in the history itself) would be a clear gain.

**Effort: S.** **Type: portable pure function**, and the test file `mergeMessage.test.ts` gives the cases to reuse.

## 8. Check results: deduplicate reruns

`packages/source-control-core/src/server/pullRequestChecks.ts` (1-79) handles CI rollups that repeat a check during
reruns. It keys each check by workflow name and check name, keeps the newest by timestamp (with careful comparison
of sub-second precision, 9-37), and disambiguates same-named checks from different workflows as `workflow / name`
(53-79). Failed and cancelled checks sort first.

**Why it helps Faber.** Only useful once Faber shows CI status to the user. Not a priority, but a correct, small pure
function if it is needed.

**Effort: S.** **Type: portable pure function.**

## 9. Watching a pull request and waking the agent

`docs/user/source-control.md` 287-303: the agent can call `watch_pull_request`. While the thread is active, the server
checks the PR every two minutes and wakes the agent on a failed check, all required checks passing, a comment or review
from someone else, or a branch that starts to conflict. Own comments do not wake it. Watching stops on merge or close,
after 10 wakes that only bring comments, after 8 failed reads in a row, or on Stop. On GitHub it first asks whether
anything changed (conditional requests) and only reads the PR then, to stay inside rate limits. Watches for the same PR
in one project share a single check.

**Why it helps Faber.** A non-programmer wants "tell me when the review needs me". The stop conditions (wake caps,
failure caps, shared watches) are the part worth copying.

**Effort: L** (needs a server loop and a host API). **Type: idea.**

## 10. Source-control abstraction across hosts

`SourceControlProvider` (`packages/source-control-core/src/server/SourceControlProvider.ts`, 116-215) is an Effect
`Context.Service` with one required set of operations (list, get, create, checkout change requests, clone URLs, default
branch) and optional hooks for host differences (`headBranchProbe`, `refineRepositoryIdentity`, `resolveLink`,
`createsChangeRequestsIn`). `SourceControlRateLimit.ts` and `driver.ts` sit beside it. Each host is a separate package
that implements it. Rate-limit handling is shared, not copied per host.

**Why it helps Faber.** Faber only needs one or two hosts at first, so a full abstraction is premature. The lesson is
the split: operations every host has, plus optional hooks for host quirks, and one rate-limit module.

**Effort: L** if built now; defer. **Type: idea.**

## 11. Things not to copy

- `restoreCheckpoint`'s `git clean -fd` without a confirmation and a pre-restore checkpoint (section 4).
- A checkpoint per turn with no pruning. Disk grows with every turn; pair it with the cleanup rules (section 5).
- Writing marker files into the user's worktree (`.t3/`). Faber should keep its own state outside the copy, or add
  `.t3/` to the ignore list of the copy.
- The Windows junction workaround is specific to a Git version. Faber should check the Git version before reusing it.

## 12. Top ideas for Faber, ranked

1. Per-turn private-ref checkpoints built from a temp index, so undo and "what changed this turn" never touch the user's branch or index (section 2). **M.**
2. Turn-change list that keeps only the files the turn authored, dropping pull and rebase imports (section 3). **S-M.** Pure parsers are portable.
3. Worktree cleanup policy: per-project rules, "never remove with local changes", orphan report with sizes (section 5). **M.**
4. Windows-safe worktree removal: mirror Git's refusals, delete with `rm` (junction-safe), restore `.git` on partial failure (section 5). **M.** Portable logic, MIT notice needed if copied.
5. One phased action for commit, push and PR, with progress per phase and plain-words errors; `sanitizeBranchFragment` for branch names (section 6). **S-M.**
6. Hide agent co-author and "Generated with" lines in the Faber UI, not in history (section 7). **S.** Portable `removeAgentCredits`.

Honourable mention: the restore gate (`ready` only) and pre-restore safety checkpoint (section 4). Build this into idea 1 from day one.

## 13. What was read, and what was not

Read in full or in the relevant ranges: `CheckpointService.ts` (checkpoint capture, restore, ref naming), `GitVcsDriver.ts`
checkpoint section 814-1381 and helpers 600-800, `Diffs.ts`, `GitVcsDriverCore.ts` create and remove (3640-3800, 4180-4231),
`worktreeRemoval.ts`, `ThreadLaunchService.ts` 230-300, `storageCleanup.ts` (selected ranges), `GitManager.ts`
`runStackedAction` 3018-3110, `GitWorkflowService.ts` outline, `packages/shared/src/git.ts` 1-40, `source-control-core`
`mergeMessage.ts`, `pullRequestChecks.ts`, `diffFileRevisions.ts`, `SourceControlProvider.ts` interface,
`scripts/setup-worktree.ts`, `docs/user/source-control.md`, `docs/user/project-settings.md` (worktree sections),
`docs/internals/pull-request-file-revisions.md`.

Not read in depth: `CheckpointDiffQuery.ts` (only its call to `listAuthoredPaths`), the GitHub provider internals
(`GitHubPullRequestProvider.ts`, `githubStackActions.ts`, `githubStackRebase.ts`), the GitLab, Bitbucket, Azure DevOps,
Forgejo and GitCafe providers (file list only), `apps/server/src/mcp/toolkits/worktree/tools.ts`, and
`packages/contracts/src/t3ProjectFile.ts`. The `GitVcsDriver.ts` helpers 600-800 were read only to the extent the
checkpoint functions call them.

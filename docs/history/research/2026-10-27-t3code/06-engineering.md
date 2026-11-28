# T3 Code: engineering tricks for Faber

Read-only study of the T3 Code clone (MIT) at
`t3code/`.
Nothing was run, built or installed there. Every idea below is a pattern to re-create in Faber's own code
(Faber's rule: original code only; reusing ideas is fine, copying is not).

Effort scale: S = under a day, M = a few days, L = a week or more.

---

## 1. Agent instructions as one budgeted source

**What T3 does.** `AGENTS.md` is the only agent guide; `CLAUDE.md` contains just `@AGENTS.md`, and `.claude/skills`
points to `.agents/skills`. Cursor-only notes live in `.cursor/rules/cursor-cloud.mdc`. The guide carries
hard "do not" rules with the reason next to each one ("The three ways to hurt yourself": no `pkill -f`, never touch
the user data dir, never set the dev URL variables). It has a "Hit every surface" checklist for any change that
spans web, desktop and mobile, and multi-provider changes. A CI job measures the agent context and fails a PR that
raises any value in `agentContextBudget.json` above `main`. A unit test keeps that file equal to the measured
context, so the context cannot grow by editing the file. A maintainer label accepts growth on purpose.

**Why it helps Faber.** `AGENTS.md` in Faber is already long and every agent (Claude, Codex, lanes) reads it on
every turn. Without a budget it only grows. A context-size guard in CI turns "keep it short" (Faber's own rule in
AGENTS.md) into a check. The "hit every surface" checklist maps well to Faber's "before a change a person will
see" section.

**Effort.** S (budget file + a jq script in one workflow) to M (the self-measuring test).

**Files.**
- `.../t3code/AGENTS.md`
- `.../t3code/CLAUDE.md`
- `.../t3code/.github/workflows/agent-context-budget.yml`
- `.../t3code/apps/server/src/provider/agentContextBudget.json` (the budget values; the test that keeps it honest is in the same folder)
- `.../t3code/.cursor/rules/cursor-cloud.mdc`

---

## 2. Custom lint rules that enforce test hygiene and project law

**What T3 does.** `oxlint-plugin-t3code/` holds about 35 custom rules, each with its own test file, wired in
`oxlint-plugin-t3code/index.ts` and named `t3code/<rule>`. Examples relevant to Faber:
- `no-wall-clock-in-tests`, `no-real-time-wait-in-tests`, `no-polling-loop-in-tests`: tests cannot sleep, poll a
  clock or wait on real time; they must use a fake timer or an auto-retrying assertion.
- `no-test-timeout-or-unreasoned-skip`: no per-test timeouts and no skip unless the skip carries a tracking number
  (`// quarantine: #1234 <why>`). `require-suppression-reason` does the same for any lint suppression.
- `no-frame-waits-in-browser-tests`: frame and transition waits race the renderer; use auto-retry or fake timers.
- `no-low-value-comments`: rejects comments that repeat the code, divide sections, or narrate tests, and caps
  a comment at four lines ("comments grew back as long why-blocks; past this, the reason needs cutting").
- `no-em-dash-copy`, `no-manual-memo` (the React Compiler memoizes for you; a hand-written memo hides bailouts),
  `no-rpc-permission-bypass`, `no-raw-mcp-registration`, `no-test-in-loop`.
- `react-effect-budget`: reads the per-file budget in `scripts/effect-budget.json` and reports when a file exceeds it
  (see idea 4).

Each rule's message says what to do instead, which matters for agents: the agent reads the lint output and fixes
the code in one step.

**Why it helps Faber.** Faber's test rules (no sleeps, no wall-clock in unit tests, "unit tests never spawn
processes") are currently prose in AGENTS.md. Agents break prose rules; a lint error they can read is enforced on
every save. The "skip only with a tracking number" rule fits Faber's review culture too. The four-line comment cap
directly targets agent-written essays in code.

**Effort.** M. Faber's tests are Rust (`cargo t`) and TypeScript (`tests/ui`, Vite). The TypeScript rules port to
any oxlint or ESLint setup. Start with three rules: wall-clock, polling loop, and unreasoned skip. Each rule
ships with a test file, so the rule itself is checked.

**Files.**
- `.../t3code/oxlint-plugin-t3code/index.ts` (rule registry)
- `.../t3code/oxlint-plugin-t3code/rules/no-wall-clock-in-tests.ts` and `.test.ts`
- `.../t3code/oxlint-plugin-t3code/rules/no-polling-loop-in-tests.ts`
- `.../t3code/oxlint-plugin-t3code/rules/no-test-timeout-or-unreasoned-skip.ts`
- `.../t3code/oxlint-plugin-t3code/rules/no-low-value-comments.ts` (the four-line cap is `MAX_COMMENT_LINES`)
- `.../t3code/oxlint-plugin-t3code/rules/react-effect-budget.test.ts`
- `.../t3code/docs/internals/testing.md` (the test rules, "AGENTS.md and this file must change together")

---

## 3. One affected-checks runner shared by local and CI, with a pass cache

**What T3 does.** `scripts/check-changed.ts` (`vp run check:changed [ref]`) diffs `HEAD` against the merge base
with `origin/main`. Uncommitted and untracked files count. It maps changed files to affected packages with
`.github/scripts/ci-changes.cjs`, the same mapping CI uses to gate its jobs, so local and CI cannot disagree.
Then it runs in parallel: patch check, format of changed files, lint of changed code files, knip (only when code
or a manifest changed), GraphQL generation check, typecheck of affected packages and dependents, and only the test
files beside changed sources (`foo.ts` runs `foo.test.ts`). The script explains why it does not use Vitest's import
graph: one leaf file pulled in 252 of 406 server tests and took seven minutes. The script hashes the diff, the raw
bytes of changed files and the relevant ignored generated files (`.generated/`, `*.d.ts`, `.env*`), and caches a
pass under that key. A rerun on identical inputs prints the last passing summary and exits. The key is recomputed
after the run; if files changed during the run the pass is not cached. It also states which heavy CI jobs remain
("CI also runs: full test suites, build, rust ...").

`scripts/test-cached.ts` (`vp run test:cached`) adds the same idea to tests: it selects test files, runs them
through a remote task cache, and with `--verify-hits N` re-executes N random cache hits without the cache. A cached
pass that fails when re-run is a verification failure. Hits are chosen by a hash of the CI run id, so the sample
changes per run.

**Why it helps Faber.** Faber's `cargo t` already runs only dependent tests, but the agent loop needs a single
"what did my change break, cheaply" command that matches exactly what CI gates. The explicit line "CI also runs:
..." tells the agent what it has not proven yet, which fits Faber's "say what is not verified" rule. The
recompute-after-run guard prevents caching a pass for files that changed mid-run, a bug agents hit often when
several lanes share a working tree.

**Effort.** M for the local runner (one script, a mapping file shared with the CI gate). L for a remote cache with
audit sampling (T3 runs a Cloudflare Worker with D1 and R2; see `infra/task-cache/README.md`). Start local only.

**Files.**
- `.../t3code/scripts/check-changed.ts` (the runner; `inputKey()` is the cache key)
- `.../t3code/.github/scripts/ci-changes.cjs` (the shared file-to-job mapping, `decide()`)
- `.../t3code/scripts/test-cached.ts` (sampled audit of cache hits via `--verify-hits`)
- `.../t3code/infra/task-cache/README.md` (remote cache design, limits, the "saved time is not money" caveat)

---

## 4. Performance budgets written as tests and checked in CI

**What T3 does.** Three layers, each a plain file plus a test:
- `scripts/effect-budget.json` lists, per file, how many React effects it may hold (for example
  `apps/mobile/src/components/VideoPreviewModal.ios.tsx: 6`). The lint rule `t3code/react-effect-budget` reads it
  and fails a file that exceeds its count. Raising a count is a visible edit in review.
- `scripts/render-metrics.ts` runs fixed UI scenarios in a test project (`renders`), counts React commits and
  component renders per scenario, and runs them three times by default. It fails unless all three runs give
  identical counts (a nondeterministic measure is a broken measure). `compare <before.json> <after.json>` prints a
  markdown table of deltas per scenario and the components whose counts moved most. Its doc says how to compare two
  refs: run `run` in a second worktree of `main`, then `compare`.
- `docs/internals/performance-regressions.md` documents the v2 wire-protocol perf command, payload byte and row caps,
  a transfer-budget integration test, and the rule that a perf claim needs a number from the same harness.

**Why it helps Faber.** Faber's rule is that every change reports its memory and processor cost and a 500 MB idle
ceiling is enforced by an end-to-end test (R125). The render-metrics idea is what is missing: a change that adds
renders or effects to a hot screen is invisible until someone feels it. A per-file budget plus a determinism check
turns "lightest and most responsive" into a diff a reviewer sees. The two-checkout compare fits Faber's worktree
lanes well.

**Effort.** M. The effect-budget file is S; a render-count harness for Faber's SolidJS screen (`ui/`) is M, since
SolidJS has no React commit counter. For SolidJS, count reactive computations or DOM node writes per scenario
instead, and keep the same "run three times, must match" check.

**Files.**
- `.../t3code/scripts/effect-budget.json`
- `.../t3code/scripts/render-metrics.ts` (`nondeterministic()` and `renderComparison()` are the core functions)
- `.../t3code/oxlint-plugin-t3code/rules/react-effect-budget.ts` and `.test.ts`
- `.../t3code/docs/internals/performance-regressions.md`

---

## 5. Evals for model-facing text, treated as regression tests

**What T3 does.** `evals/` is a separate package (`evals/package.json`, one folder per eval, graders run with
`vp test run`). The rules in `evals/README.md`:
- Real models on real text. No mocks of the model output.
- Add a failing case before the fix to model behaviour (AGENTS.md repeats this rule).
- Several samples per case. A case fails when half or more of its samples fail, so a flaky model does not flip the
  result.
- Compare against a baseline on the same model, not an absolute score. `--stop-on-regression` stops the run when
  a case falls below its baseline.
- Case kinds: `bad` (must not happen), `guard` (a rule that must hold), `control` (a normal case that must still
  work). A pure "bad" suite can be gamed by refusing everything; the controls catch that.

Two concrete evals:
- `evals/thread-titles/`: generates a conversation title with the real text model, then a blind A/B review in
  random order against a baseline title, scored by a per-case rubric. Cases include long-message truncation and a
  scope change in the middle of a thread (`cases.ts`, `previousTitle`).
- `evals/mcp-tool-use/`: replays fake threads recorded from real harness captures, with a cheap shell-simulator
  model answering tool calls. Cost is documented per run.

**Why it helps Faber.** Faber sends prompts to CLIs whose output it then shows (chat titles, the helper that picks
a model, the "stuck helper" wording, the AI-written what's-new text). Prompt edits currently change behaviour with no
signal. The bad/guard/control split is the part Faber most needs: a prompt change that makes an agent refuse more
work is a regression that a single "good output" check misses.

**Effort.** M for one blind A/B eval over a handful of cases. L for the replay harness with recorded captures.

**Files.**
- `.../t3code/evals/README.md`
- `.../t3code/evals/thread-titles/README.md`, `cases.ts`, `run.ts`
- `.../t3code/evals/mcp-tool-use/README.md`
- `.../t3code/evals/package.json`

---

## 6. Release flow: nightly is the candidate, a preview train, and a smoke test on an old database

**What T3 does.** `docs/operations/release.md` and `.github/workflows/release.yml`:
- A manual stable release builds the commit of the latest published nightly, not `main` HEAD. Nightly is the
  release candidate: verify it, then promote it. Merges keep landing without leaking into the stable build.
- A `preview` channel publishes the triggering commit as a GitHub prerelease with a `preview` version suffix. It
  exercises the full flow (build, sign, notarize, smoke, publish) for unmerged work, carries no updater manifest
  (so no install is offered it), and is installed only by name. The release body is a warning, not generated notes,
  because generated notes of unmerged history are not a changelog.
- `scripts/smoke-desktop-app.ts` opens the packaged app under Xvfb against a seeded database. The database
  in `scripts/fixtures/desktop-launch` is an old snapshot on purpose: the packaged server must migrate it forward, as
  upgrading users do. The doc says not to regenerate it on schema changes.
- The bundle is built once (`build_bundle`) and handed to all platform jobs as an artifact, so no runner rebuilds
  it. Packaging jobs only package.
- Each CLI archive is extracted and executed on its own build runner before publish (`scripts/smoke-cli-archive.ts`).
- Stable tags with a suffix become GitHub prereleases; only plain `X.Y.Z` is marked latest. Release notes compare
  to the previous tag in the same channel.
- Runtime changes need a new version number because installed runtimes are cached by version.

**Why it helps Faber.** Faber's release policy (releases only for finished areas) and its self-updating install at
`E:\Apps\Faber` need a pre-release lane that does not reach the updater. The old-database smoke test is the cheapest
way to catch a broken migration, which is the failure that hurts upgrading users most. "Build once, package
many" removes repeated build time from every platform job.

**Effort.** M for the smoke test on an old fixture database and the "build once" artifact hand-off. L for the full
nightly-to-stable promotion and the preview train.

**Files.**
- `.../t3code/docs/operations/release.md`
- `.../t3code/.github/workflows/release.yml`
- `.../t3code/.github/workflows/release-desktop.yml`
- `.../t3code/scripts/smoke-desktop-app.ts`
- `.../t3code/scripts/fixtures/desktop-launch/` (the old-schema snapshot)
- `.../t3code/scripts/smoke-cli-archive.ts`

---

## Other ideas worth a later look

- **License notices generated per client.** `scripts/sync-third-party-license-notices.ts` is a 10-line wrapper around
  `scripts/lib/third-party-licenses.ts`. Web gets a JSON file; mobile gets a Metro virtual module. A strict build
  fails when a dependency's license text is missing. Overrides and custom notices live in
  `third-party-licenses.config.json`, and the SPDX template cache sits in `.generated/`. Fits Faber's rule against
  bundling third-party tools: the check proves what ships. Effort S-M.
  Files: `.../t3code/docs/internals/open-source-licenses.md`, `.../t3code/scripts/sync-third-party-license-notices.ts`,
  `.../t3code/scripts/lib/third-party-licenses.ts`.
- **PR size labels from effective changed lines.** `.github/workflows/pr-size.yml` labels `size:XS` (0-9 changed lines),
  `S` (10-29), `M` (30-99), and so on, with test files excluded from mixed PRs. Gives the review queue a size signal
  without anyone counting. Effort S.
  File: `.../t3code/.github/workflows/pr-size.yml`.
- **Self-test for the live verifier.** `docs/operations/background-verification.md` describes a verifier with a
  `--fail-gate` self-check: the gate itself is proven to exit nonzero on a known-bad input. Effort S.
  File: `.../t3code/docs/operations/background-verification.md`.
- **Knip with per-workspace entry points and documented ignore reasons.** Each `ignoreDependencies` entry carries a
  reason, and `includeEntryExports` is set for packages. Effort S.
  File: `.../t3code/knip.jsonc`.
- **Contribution triage with policy loaded from the base branch.** `.agents/skills/contribution-triage/SKILL.md` loads
  its policy from `refs/heads/main` at a fixed SHA, not from the PR, so a PR cannot weaken its own review rules; it
  fails closed when a policy file is missing or malformed. Relevant to Faber's lane PRs that must not edit their own
  gate. Effort M.
  Files: `.../t3code/.agents/skills/contribution-triage/SKILL.md`, `.../t3code/.github/TRIAGE_EXEMPTIONS.td`.
- **Telemetry off in dev and CI.** `docs/internals/product-analytics.md`: the dev runner and CI smoke run with
  telemetry off, so test servers never count as installs. Faber already defaults telemetry off; the useful part is the
  dev-runner and CI override. Effort S.
  Files: `.../t3code/docs/internals/product-analytics.md`, `.../t3code/scripts/dev-runner.ts`.

---

## Top six, in order of value for Faber

1. Idea 3: one affected-checks runner that uses the same file-to-job map as CI, with a pass cache keyed on the diff and file bytes.
2. Idea 2: three custom lint rules first (wall-clock, polling loop, unreasoned skip), each with its own test file.
3. Idea 1: an agent-context budget enforced by a CI job that fails growth.
4. Idea 4: per-file budgets plus a run-three-times determinism check for UI cost.
5. Idea 5: blind A/B evals with bad/guard/control cases for prompts Faber sends to the CLIs.
6. Idea 6: nightly as release candidate, a preview channel for unmerged work, and a packaged-app smoke test on an old-schema database.

---

## Coverage and limits

- Read in full or in the relevant part: AGENTS.md, CLAUDE.md, CONTRIBUTING.md (summary), the `.agents` contribution-triage
  skill, `.cursor/rules/cursor-cloud.mdc`, `docs/internals/testing.md`, `performance-regressions.md`,
  `product-analytics.md`, `open-source-licenses.md`, `docs/operations/release.md`, `background-verification.md`,
  `evals/` (README, thread-titles, mcp-tool-use, package.json), the oxlint plugin rule list and samples, `knip.jsonc`,
  `scripts/check-changed.ts`, `scripts/test-cached.ts`, `scripts/render-metrics.ts` (first 120 lines),
  `scripts/effect-budget.json` (first 60 lines), `infra/task-cache/README.md`, `.github/workflows/agent-context-budget.yml`,
  `pr-size.yml` (first 40 lines) and the job list of `ci.yml`.
- Not read in detail: `docs/operations/development.md`, `docs/operations/observability.md`, `scripts/dev-runner.ts`,
  `scripts/sync-reference-repos.ts`, most of `ci.yml`, `release-desktop.yml`, and the `infra/task-cache/src` code.
  The claims about those files are taken from the docs that point to them, not from the code.
- No code was run. The "saved time" and performance numbers quoted are the repository's own documentation claims.

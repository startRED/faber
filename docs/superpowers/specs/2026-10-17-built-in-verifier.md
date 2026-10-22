# Faber's built-in verifier: checks in seconds, on the PC, inside Faber

Date: 2026-10-17. Status: approved by Erick on 2026-10-17; V0 measured the same day
([results](../../research/2026-10-17-verifier-v0-measurements.md)), V1 is next. Direction and the macOS choice are his
(R190); the details are the AI's. Research: five web surveys on 2026-10-17 (one Sonnet, four Haiku), sources in
[Sources](#sources). Several numbers there come from vendors or secondary write-ups and are marked as such; step V0
measures the ones this plan depends on.

## 1. Goal

Erick, 2026-10-17: Faber gets its own verifier, built for Faber's workflow, which Faber uses to build itself and later
offers to every project. It may live inside Faber's own process when that is the most efficient place. Like the CLIs, it
stays outside the 500 MB budget (R125), but it is measured and shown on its own. It is written in Rust.

The aim is the fastest and lightest verification possible, not a copy of a hosted CI:

- **An agent learns whether its change works in seconds, in its own chat**, before any push. GitHub stops being the place
  where errors are found and becomes the final confirmation.
- **Idle cost is zero**: no extra process, no CPU, a few MB in the engine.
- **Nothing leaks**: RAM, CPU, VRAM, handles, processes, ports, disk and threads return to their idle level after every
  run, after a thousand runs, and after a crash ([section 5](#5-no-leaks-by-construction)).
- **It never disturbs the person**: no window, no focus taken, no stutter in a full-screen game.

## 2. Where we start

| Fact | Source |
| :--- | :--- |
| The repository is public, so GitHub's standard runners cost no minutes. The limit is how many jobs run at once. | `gh repo view`, 2026-10-17 |
| CI already runs only the suites a change needs (R163, R165), and a pull request runs only the lints (R186). | [dev/testing.md](../../dev/testing.md#screen-tests), [dev/workflow.md](../../dev/workflow.md#merge-on-green) |
| A queued run still holds about 18 jobs (one test-app build, one `ui` part, eight `screen` parts, lints and `rust` on Windows and macOS); five runs at once left screen parts waiting up to 226 s for a runner. | R186 |
| Local `cargo t` peaks near 8 GB, and lanes waited 441 minutes for a build slot on 2026-10-16 and 17, so the heavy suites moved to CI (R183). | [dev/testing.md](../../dev/testing.md#build-slots) |
| Missions already have a serial proof, review and fast-forward queue (R96), short check output (R155), and the rule that Faber, not the Writer, decides "done" (R146). | [product/building.md](../../product/building.md) |
| The test-system rework (R113, R115, R116) already plans one engine test program, event waits, skipping unchanged builds and one shared running app for the screen scenarios. | [spec](2026-10-14-test-system-rework-design.md) |

So the verifier is not a new idea next to these. It is the engine that runs them: `cargo t`'s plan, the mission proof
queue and R146's "Faber decides done" become one always-ready service in the engine, and the test rework's shared app
becomes its one long-lived worker.

## 3. What the research says

- **Nobody keeps a compiler warm.** No rustc daemon exists or is planned. Rust's 2026 goals aim at relinking without
  rebuilding (5 to 10x faster rebuilds, estimated). "Warm" means warm disk: `target/` per worktree, incremental folders,
  sccache.
- **Process launches are the cost on Windows**, more than 20x a Linux fork and exec in one benchmark, and very sensitive
  to Defender. Keep libtest's many tests per process. Use nextest's process per test only to isolate a flaky test.
- **The best systems keep the graph in a long-lived process and the heavy work in a few workers.** Nx keeps its project
  graph in a daemon fed by file events and exits after 3 idle hours. Bazel feeds long-lived workers requests over stdin,
  2 to 4x faster by its own docs. Buck2 keeps one incremental graph with early cutoff on equal values; equality bugs were
  its main defect source. Turborepo dropped its daemon because a model of the tree is only as right as its last event.
- **Selection by impact is what large teams do.** Stripe runs about 5% of tests per change. Meta's model runs about a
  third and still catches over 99.9% of faulty changes. Anthropic (September 2026) selects by past results plus package
  relevance, with no model. Its single-process selector fell behind the job flow until results moved to a journal on
  disk, rolled up every few seconds.
- **Queues that know about conflicts save the most CI.** Uber reports 53% less CI and 37% shorter waits. Mergify and
  bors batch and bisect a failed batch. Anthropic replaced its merge queue with tests after merge plus automatic bisect
  and revert. That clashes with R127's green base, so it stays out of this plan.
- **Agents bypass slow hooks.** Coder saw agents turn hooks off after a shell timeout. A check an agent runs on every
  change must take seconds; the full set runs once, before landing.
- **On Windows: the Dev Drive.** Microsoft claims up to 30% faster builds than NTFS with Defender on, and 14% against an
  exclusion. Defender's performance mode exists only there. No cargo measurement was found, so V0 measures it.
- **Tooling around the build.** TypeScript 7 (`tsgo`, general availability reported for July 2026) type-checks 8 to 9x
  faster, though its API may break typescript-eslint. Oxlint is already Faber's linter. `debug = "line-tables-only"`
  saves 10 to 20%. Cranelift breaks `#[should_panic]` on Windows, and mold and wild do not run there.
- **No local CI fits.** GitHub's runner (MIT, C#) starts a worker per job. act, Woodpecker, Dagger and Earthly need
  containers. Gitea's act_runner host mode is the closest, in Go. A verifier in Faber's engine reuses `testkit`'s plan,
  the job objects of `winproc` and the engine's event channel, so building our own is smaller than adapting any of them.
  Everything above is open source, so no reverse engineering is needed. Faber borrows ideas only, never code. bacon is
  AGPL, so not even as a dependency.

## 4. Design

### Processes

| State | Processes | Where the state is |
| :--- | :--- | :--- |
| Idle | **0 extra.** The verifier is a module of the engine (a tokio task set), so it moves with the engine when the engine becomes its own process. | Graph and queue in RAM (bounded, rebuildable); results journal on disk |
| Checking Rust | `cargo` and the `rustc` and linker processes it starts (capped at half the cores, as today), then **one** engine test program | `target/<worktree>`, incremental, sccache |
| Checking the screen | The one shared test app (the test rework's step), kept alive between checks and fed requests over stdin; `vite` only when the UI changed | The app's hidden desktop and profile, reset between requests |
| Checking TypeScript | `tsc.exe` (TypeScript 7 is native) and oxlint, started directly, never through `npm` or `npx` | Their own incremental files |

Every child starts inside **one Windows job object per run**, with no console window (`CREATE_NO_WINDOW`) and on the
hidden desktop when it has a window. Closing the job kills every process in it, grandchildren included.

### The parts in the engine

1. **Change feed.** One recursive watcher per worktree (ReadDirectoryChangesW through `notify`) that ignores `target/`,
   `node_modules/` and `.git/objects`. When the watcher overflows, Windows drops pending events, so the feed falls back
   to `git status` for that worktree, as Watchman does. Edits are batched for 300 ms, longer while an agent is still
   mid-turn.
2. **Graph.** Content hash of each file, then its crate or package, then the reverse dependencies, then the suites and
   tests. It is built from `cargo metadata`, the TypeScript import graph and `testkit/suites.json`, the same rules as
   `cargo t` and `scripts/ci-scope.mjs`, so the two never disagree. A manual list covers what imports cannot show, such
   as fixtures, locales and `product-map.md`. Early cutoff: a file saved with the same content triggers nothing.
3. **Scheduler.** One heavy Cargo run at a time across all worktrees, plus light checks. A new change to a worktree
   cancels its running check at once by closing the job. Admission is by free RAM, not by slots: a run starts only when
   its expected peak, learned from its last runs, fits under the budget, so the RAM budget replaces today's three slots.
   It is the same rule the roadmap gives CLI workers.
4. **Two levels of check.**
   - **Fast**: on every change an agent finishes. Only the selected tests, failed and flaky first. Target: under 30 s for
     one changed engine file on a warm worktree.
   - **Full**: once, on the exact tree that will land. Every suite the plan names, and never a selection. This is the
     one that counts.
5. **Results.** Typed engine events (`verify.started`, `verify.step`, `verify.done`) with i18n keys. Each failure comes
   as R155's short form: file and line, the tool's fix, at most 20 failures of 8 lines. The whole output streams to a
   file on disk and never sits in RAM. Agents get a `verify(worktree)` tool in Faber's toolbox that returns the summary.
   The Projects chat shows the result inline. The screens are designed in V5 with the product map, not here.
6. **Journal.** Append-only result history (test, tree, outcome, time, peak RAM) in an embedded store. It is rolled up
   every few seconds and keeps the last 50 results per test. It feeds flaky detection, failed-first ordering and run
   cost estimates. A crash loses nothing that was written, and the graph rebuilds from the journal and git at start.

### GitHub: confirmation, not discovery

- When a full check passes on the exact tree `land.mjs` squashed, the verifier posts the commit status `faber/verify` on
  that commit, using the person's own `gh` sign-in. Faber never reads a CLI credential (AGENTS.md).
- The queue's CI run reads it. Its `changes` job sees `faber/verify` green for that tree hash and leaves the heavy suites
  out, the way R165 already leaves out suites a change does not need. The gate stays green, and the lints still run.
  The tree hash is compared, not the commit id, because the squash makes a new commit.
- **What stays on GitHub:**
  - every suite once a day on `main`, as today, which also measures the selection's miss rate against the full set;
  - every suite on a commit before it becomes stable (`promote-stable.yml`);
  - macOS. The PC proves macOS only through Clippy (R166). Erick chose, on 2026-10-17, to run it once a day and before
    each release, no longer on every queued commit, from V2 on ([section 8](#8-for-erick-to-decide)).
- **Trust.** A status is a string on a commit: anyone with write access could post one. Only the owner's PC posts it,
  through its own sign-in, and the daily full run catches a false green within a day.

## 5. No leaks by construction

Each limit below is enforced by code and proved by a check in V1, so a leak fails a test instead of being found by the
person.

**RAM, engine side**
- Every queue and channel is bounded: tokio bounded `mpsc`, file events coalesced per path, and a full queue merges
  requests instead of growing.
- Per-worktree state is dropped when its worktree goes. The graph is capped by file count. The journal keeps a bounded
  window and is compacted.
- Tool output is never buffered whole. It streams to disk, and only the parsed failures, at most R155's limits, stay in
  memory until read.
- **Check:** the engine with the verifier idle adds at most 15 MB over the engine without it. After 200 fast checks on a
  fixture repository, private memory is back within 5 MB of the level after the first check.

**RAM, child side**
- The job object caps the run's memory (`JOB_OBJECT_LIMIT_JOB_MEMORY`) at the admitted budget. A run past it is ended
  and reported, never left to push the PC into paging.
- The shared test app restarts after N requests or when its memory passes a cap, and exits after 10 idle minutes.
- Background runs ask Windows for low memory priority, so their pages are trimmed before the person's.

**CPU**
- Idle is truly idle: no polling and no timers except the idle exits; file and process events wake it.
- Runs use the below-normal priority class, low I/O priority and EcoQoS (Windows 11 power throttling), so they prefer
  efficiency cores and leave the person's apps responsive.
- **Game guard.** When a full-screen or Direct3D app is in front (`SHQueryUserNotificationState`), no new run starts and
  the job's CPU rate is capped (`JOB_OBJECT_CPU_RATE_CONTROL`, about 10%), until the person leaves the game.
- A superseded run is killed at once, never left to finish.
- **Check:** 10 idle minutes add 0 CPU seconds, measured by the job-object meter of R113.

**VRAM**
- The engine and the toolchain use no GPU.
- The shared test app runs WebView2 with GPU off (software rendering), so tests hold no VRAM and never compete with a
  game. Software rendering is also steadier for screenshots. Any scenario that truly needs the GPU is named and runs
  only in CI.
- **Check:** the test app's processes show no GPU process and no dedicated GPU memory.

**Processes, handles, ports, threads**
- `KILL_ON_JOB_CLOSE` on every run's job: if Faber crashes, Windows kills its children, so no orphans. The audit `cargo t`
  already does (leaked processes, writes outside the sandbox) runs after every check.
- Child pipes are read concurrently until they close (a full pipe would hang the child), and process and job handles are
  closed on exit. The DevTools port is picked fresh and dies with its job.
- The work runs on the engine's tokio runtime and its blocking pool, so a verifier thread pool never outlives a run.
- **Check:** after each run the job is empty, the engine's handle count (`GetProcessHandleCount`) and thread count are
  back to their level before the run, and no process of the run is alive.

**Disk**
- `target/` keeps R106's sweep. sccache gets a size cap. The journal is compacted. Run logs keep the last 20 per
  worktree.
- Temporary sandbox homes are removed after each run, and leftovers are swept when the engine starts.

**Correctness, the leak nobody sees**
- A wrong "nothing changed" is the worst bug, Buck2's lesson. Every cutoff is a content hash, never a timestamp or a
  cached guess. A watcher overflow forces a rescan. The full check before landing never uses the selection.
- The daily full run on GitHub compares itself with the selections of that day and reports every test the selection
  missed. Above zero misses, the selection rules get the fix before anything else.

## 6. Steps

Each step lands on its own, proves itself with the checks above, and reports its memory and CPU cost.

- [x] **V0. Measure first, no product code.**
  - Time a cold build, a one-file rebuild and the rust suite on NTFS with Defender exclusions, then on a Dev Drive with
    performance mode.
  - Time `tsc` against `tsgo`, and check that the lint still works with `tsgo`.
  - From GitHub's API: how long the last 50 queued runs waited for runners, ran and went red, and which job failed.
  - Result: a dated page in `research/` and a go or no-go for each speed item.
  - Measured ([results](../../research/2026-10-17-verifier-v0-measurements.md)): CI jobs waited 3,133 minutes for a
    runner and ran 1,854; `tsgo`, light debug info and `rust-lld` were already in; Defender costs nothing in `target/`,
    so no Dev Drive for now; a one-file Rust check takes 7 s; 22 rust tests fail in any `.claude\worktrees\` lane.
- [x] **V1. The verifier for Faber itself.** The engine module of [section 4](#4-design) running `cargo t`'s plan,
  `tsc` and `oxlint` per worktree:
  - change feed, graph, RAM admission, one job per run, game guard, journal;
  - fast and full checks, results as events and the `verify` tool;
  - every check of section 5;
  - test folders outside the worktree, so a local run agrees with CI (V0 found 22 tests that do not).
  - Lanes then see their result in seconds without a push. It replaces the three build slots.
  - Built next: RAM admission (each check's job peak goes to the journal; Cargo builds overlap only when their peaks and
    2 GB for the person fit in the free memory, an unknown one runs alone; no job memory cap yet), the results journal
    (JSON lines in the data folder, the last 50 per check; failed first, then cheapest; flaky told to the agent), the game guard (no new check while a full-screen app is in front, a running check's job capped at 10%
    of the processor), and fast's Clippy on each Rust package a change touches and on every package that depends on it
    (`cargo metadata --no-deps`, about 0.1 s).
  - Left out of V1 by measurement (AI, Erick delegated): the file watcher, since asking git costs about 0.45 s per verify on
    this repository against checks of 1 to 60 s, while a watcher holds a thread and handles per open project for good;
    and the TypeScript import graph, since the fast checks run the project's own type check and lint, which read the
    whole program, and no generic per-file test runner exists to select from. Both come back if a measurement shows
    the git call or a whole-program check is what a fast verify waits on.
  - Built first (R203): the `verify` tool, fast and full, on the project's own Checks of R137 (no second checks file),
    selection found from each command, content-hash cutoff, one kill-on-close job per check below normal priority, one
    Cargo build at a time, logs on disk; the change feed is git asked at each call, with no watcher yet.
  - Built last (R429 of `docs/next/verify-limits.json`): the job memory cap, half again a check's kept peak or the
    room past the reserve when more, at least 1 GiB, none before a first measured run; a failure at it is told to the agent. Left out by evidence: low memory priority, EcoQoS and low
    I/O priority (per process on Windows, with no inheritance or job-wide class, so the tree would need watching); the
    `verify.*` events move to V5, whose screens are their only reader.
  - The checks of section 5: `engine/tests/verify_cost.rs` measures the idle verifier (at most 15 MB) and the memory
    after 200 fast checks (within 5 MB of the first), and `engine/tests/verify.rs` the cap. Idle CPU stays zero by
    construction: the verifier has no thread, timer or watcher of its own, and the game guard reads only while a run waits.
- [x] **V2. GitHub confirms.** Built: `land.mjs` runs the suites CI would run on the exact queued commit with `cargo t`
  (the project's own runner; the engine's verifier runs the proof commands, not the screen suite) and posts
  `faber/verify`; ci-scope.mjs takes it only for the named tree, from the owner. Misses show as a red daily run on main.
- **V2 as planned:** `land.mjs` asks the verifier for a full check on the squashed tree and posts
  `faber/verify`. The `changes` job leaves out the suites the status covers. The daily run reports misses.
  - Result: most queued runs shrink from about 18 jobs to the lints.
- [x] **V3. A queue that knows conflicts.** Built in `land.mjs` and the queue jobs of ci.yml, not in the engine (R220).
  - Changes that touch disjoint crates are checked together.
  - A failed batch is bisected.
  - Flaky tests are found from the journal, quarantined and rerun alone. Built for whole checks: a check the journal
    knows as flaky (a kept pass and a kept failure on the same content) that fails runs once more on its own, and only
    its second result counts.
  - `land.mjs`'s queue moves into the verifier, and R96's mission proof queue uses the same service.
- [ ] **V4. One shared test app.** The test rework's shared running app becomes the verifier's long-lived worker:
  - GPU off, requests over stdin, reset between requests;
  - restart caps and idle exit;
  - screen checks lose their per-scenario launch (about 6 WebView2 processes each).
- [ ] **V5. Every project (Phase 4).**
  - The same service for a user's project, with the checks the project declares (package scripts, Cargo, pytest, go
    test) and R155's readers.
  - Selection grows per language from the import graph.
  - A remote machine over SSH can run the children instead of the person's PC.
  - The screens, settings and words go through product-rules.md and the product map at that step.

## 7. Risks

1. **False green** from a selection bug or a status on the wrong tree. Mitigated by the tree hash, the full check before
   landing and the daily full run with its miss report.
2. **Windows watcher gaps** (overflow, network drives, renames). Mitigated by the `git status` rescan and by content
   hashes.
3. **Contention.** Several worktrees building at once fight over CPU, disk and Cargo's lock. Mitigated by one heavy Cargo
   run at a time and RAM admission.
4. **Defender stalls** on fresh test binaries. Measured in V0. A Dev Drive or an exclusion is a system setting, so
   Erick's.
5. **The engine's own weight.** Mitigated by the idle check of section 5, which fails above 15 MB.
6. **macOS** is proved only by Clippy on the PC ([section 8](#8-for-erick-to-decide)).

## 8. For Erick to decide

1. **The direction** (R190): approved by Erick on 2026-10-17.
2. **macOS on GitHub**: decided by Erick on 2026-10-17. It runs once a day and before each release, starting with V2.
   R166 already catches most macOS breaks on the PC.
3. **A Dev Drive** with Defender's performance mode for `target/`, the Cargo home and sccache, if V0 shows the gain. It
   is a system setting, which only Erick changes.
4. **Not recommended: tests after merge** (Anthropic's way). It would undo R127's green base. Kept out unless he wants
   it.

## Sources

- Anthropic, test impact analysis (2026-09-14): claude.com/blog/agentic-coding-is-straining-ci-heres-how-we-scaled-test-impact-analysis-at-anthropic
- Anthropic, Scion source control (2026-10-13): anthropic.com/engineering/redesigning-source-control
- Uber, CI costs (2025-06-23): uber.com/blog/slashing-ci-costs-at-uber; Meta, predictive test selection (2018): engineering.fb.com/developer-tools/predictive-test-selection
- Bazel persistent workers: bazel.build/remote/persistent; Buck2 daemon and DICE: buck2.build/docs; Nx daemon: nx.dev/docs/concepts/nx-daemon
- Rust fast-builds goals 2026: goals.rust-lang.org/2026/roadmap-fast-builds.html; nextest on Windows: nexte.st/docs/installation/windows
- OS process cost: bitsnbites.eu/benchmarking-os-primitives; Dev Drive: blogs.windows.com/windowsdeveloper/2023/06/01/dev-drive-performance-security-and-control-for-developers
- Mergify batches: docs.mergify.com/merge-queue/batches; bors-ng: github.com/bors-ng/bors-ng; GitHub commit statuses: docs.github.com/en/rest/commits/statuses
- Coder hook bypass: mirrored commit dd34e3d in coder/coder; CircleCI test hooks (2026-03): circleci.com/blog/test-hooks-ai-development
- Reddit r/ClaudeCode "My next bottleneck is CI" (read through the PullPush archive): fast and slow gates, emulators, local runs

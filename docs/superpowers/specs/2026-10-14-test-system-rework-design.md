# Test system rework: beat TL-ADE's suite by 5x

Date: 2026-10-14. Status: approach approved by Erick (R113, option 1); cold-run target R114 (Erick); this spec awaits his review.
Research: TL-ADE mapping and process sources were read on 2026-10-14 (no code copied, AGENTS.md).

## 1. Goal

R113 (Erick): Faber's test system beats TL-ADE's by at least **5x in wall time and processes started** and **3x in peak
RAM and CPU**, on the whole suite and on each matched scenario. Same PC, same core budget, nobody using it. No test is
weakened or dropped to win. Behaviours Faber has no feature for are listed apart, never counted.

## 2. Where we start (measured 2026-10-14, whole-system sampler, warm)

| Run | Wall | Peak RAM | CPU | Processes started | Tests |
|---|---|---|---|---|---|
| TL-ADE `vitest run` | 667 s | 3.7 GB | 454 s | 1,795 (its own meter: about 17,000 children, 15,474 of them `git`) | 3,446 (62 failing) |
| Faber `cargo t --all` | 562 s | 3.1 GB | 425 s | 953 | 816 |
| Faber rust suite, nothing changed | 38 s | 0.15 GB | 58 s | 37 | 615 |
| Faber rust suite, one engine file changed | 62 s | 7.8 GB | 256 s | 104 | 615 |
| Faber rust suite, fresh target folder | 204 s | 5.5 GB | 875 s | 786 | 615; 8.1 GB on disk |

Where Faber's time goes: 78 screen scenarios and the UI checks sum to 485 of the 562 s. Each scenario launches the real
app (about 6 WebView2 processes) and tears it down: 48 launches, 313 WebView2 processes. The rust suite ran 27 test
programs one after another, and one changed file relinked all of them at once (33 linkers, 7.8 GB).

The sampler counted every new process on the PC, including other sessions' tools (`NVIDIA App`, hooks), so both sides
carry noise. Section 3 replaces it.

## 3. The fair benchmark

A small Rust tool, `testkit/src/bin/faber-bench.rs`, runs one command inside a fresh Windows job object (the command
starts suspended, joins the job, then resumes, so no child escapes) and reads the job's own accounting when it ends:

- wall time;
- total processes started in the job (`TotalProcesses`);
- CPU time, user plus kernel (`TotalUserTime + TotalKernelTime`);
- peak committed memory of the whole job (`PeakJobMemoryUsed`);
- bytes written to disk by the job (`WriteTransferCount`), and the size of the build folder before and after.

Same rules for both sides, written into the tool so a run cannot drift:

- the PC is idle: the tool waits until no keyboard or mouse input for 60 s and refuses to start while a full-screen app
  runs (the same Windows check the tray uses);
- the core budget is the same: half the logical cores (TL-ADE's own default with nobody at the PC; Faber's `jobs()`);
- each measurement runs 3 times and reports the median; TL-ADE varies from 489 to 707 s between runs;
- every row records the commit of each repository, the tool versions and the date, in `docs/research/bench/` as
  JSON lines, so a later run can be compared with this one.

What is compared:

| Tier | TL-ADE | Faber |
|---|---|---|
| Whole suite, warm | `vitest run` (default include, probes excluded) | `cargo t --all` |
| Whole suite, cold | fresh clone, `npm ci`, then `vitest run` | fresh clone, `cargo t --all` with an empty build folder |
| Matched scenarios (8) | the files below, run alone | the matching tests via `cargo t --filter` |
| Categories | real browser UI; spawns a fake CLI; real filesystem; pure logic | screen and UI checks; replayed sessions; real filesystem; pure logic |
| Real git | 65 files | none yet: comes in with the self-development branches |

The 8 matched scenarios, each proving the same behaviour on both sides:

| # | Behaviour | TL-ADE | Faber |
|---|---|---|---|
| 1 | A turn with a recorded CLI streams and ends | `adapter-claude.test.ts`, `fake-cli.test.ts` | `claude_session.rs`, `codex_session.rs` |
| 2 | Resume continues the same session | `adapters/codex_resume.test.ts`, `adapter-claude.test.ts` | `claude_session.rs`, `codex_session.rs`, `engine.rs` |
| 3 | Quota is read from each CLI | `official_quota.test.ts` | `quota.rs`, `budget.rs` |
| 4 | Hitting the quota pauses and resumes after the reset | `quota_pause_resume.test.ts`, `engine-quota.test.ts` | `waiting.rs` |
| 5 | The router picks a model by quota and cost | `model_routing.test.ts`, `model_chains.test.ts` | `route.rs`, `pacing.rs`, `auto.rs` |
| 6 | Unknown or blocked billing is never picked | `model_routing.test.ts`, `adapters/claude.test.ts` | `budget.rs`, `paid_guard.rs`, `pacing.rs` |
| 7 | Tokens and cost are recorded, never invented | `adapters/claude.test.ts`, `usage_by_company.test.ts` | `turn_usage.rs` |
| 8 | An npm `.cmd` shim resolves to the real program | `doctor.test.ts`, `parity/windows-durability.test.ts` | `engine.rs` |

Seven more pairs match loosely (process tree, crash recovery, atomic writes, path guards, interrupt, quota in the UI, a
chat round trip in the UI); they are reported as a second table, not counted for R113. TL-ADE behaviours with no Faber
feature (missions, git worktrees, reviewers and gates, course, media, docs tests) are listed, not measured. Faber
behaviours with no TL-ADE twin (idle memory, CLI install, notifications, long-thread UI) are listed too.

## 4. Changes

### 4.0 Done in this change, before the benchmark tool

- One test program for every engine test file (`engine/tests/all.rs`), plus `install_memory.rs` alone because it reads
  its own process memory. A test lists every file and fails when a new one is not included.
- `rust-lld` as the Windows linker; no debug info for dependencies, line tables for our own code.
- The runner runs below normal priority; everything it starts inherits it.

Measured: nothing changed 38 → 21 s; one file changed 62 → 28 s and 7.8 → 1.4 GB.

### 4.1 Screen scenarios share a running app

Each scenario declares whether it needs a fresh app. Fresh: first run and tutorial, setup, restart and resume after
reopening, containment, hidden desktop, idle memory, updater and tray. Every other scenario runs on an app that stays up.

Between two shared scenarios a test-build-only command, `test_reset`, returns the app to the state of a fresh launch:
the engine ends every CLI it started, drops its in-memory state and switches to a new empty data folder inside a new
sandbox; the page reloads. The runner then proves the reset worked before the next scenario starts, and fails the run if
not: the job holds only the app's own processes, the old sandbox has no write the scenario did not own, the new data
folder is empty, and the page shows the fresh-launch screen. The audit each scenario has today (leaked processes, writes
outside the sandbox, changed environment) still runs after every scenario.

### 4.2 Lanes in parallel

Shared scenarios run on three lanes, each its own app, sandbox and DevTools port from a range reserved per run (no more
bind-then-release race), all on the run's hidden desktop. Fresh scenarios and the ones that assert memory or timing run
alone after the lanes finish, so CPU contention cannot change their numbers. Lanes never exceed the core budget; with
fewer than 8 logical cores there is one lane.

### 4.3 Waits follow events, not the clock

About 40 fixed pauses wait for an animation or a debounce. They become waits for the event: no running animations on
the element (`getAnimations()`), two animation frames, or the app's own "settled" signal in the test build. Waits that
prove "nothing happens for N seconds" and the 10 s idle-memory settle keep their real length: shortening them would
weaken what they prove. Teardown waits for the job to report no processes instead of polling the process table.

### 4.4 Builds skip what did not change

The test build of the screen runs Vite only when `ui/`, `tests/ui/` or the Vite config changed since the last build
(content hash next to `dist-test`). The ui checks keep their single launch.

### 4.5 Cold builds reuse the cache

New worktrees (self-development) start from a copy-on-write or hard-linked copy of the main build folder's dependencies,
so a fresh worktree compiles only Faber's own crates. The build folder budget and cleanup of R106 apply.

## 5. Acceptance

R113 is met when the benchmark tool reports, as medians of 3 runs:

| Tier | Target against TL-ADE |
|---|---|
| Whole suite, warm | at least 5x less wall time and processes, 3x less peak memory and CPU |
| Each of the 8 matched scenarios | the same ratios |
| Whole suite, cold | at least 2x less wall time (Faber compile plus suite against TL-ADE `npm ci` plus suite) |

And: every Faber test that passed before still passes, with the same assertions; `cargo t --all` stays green on
Windows, macOS and Linux in CI; a forced failure inside a shared scenario still fails that scenario and does not leak into
the next one (a test proves it).

## 6. Risks

- **A reset that is not complete** makes a later scenario pass for the wrong reason. The runner's proof after each reset
  (section 4.1) is the guard, and a test plants leftover state on purpose and expects the proof to fail.
- **Flaky timing under parallel lanes.** Memory and timing scenarios run alone; a scenario that fails only in a lane is
  moved to the serial group, never retried silently.
- **`rust-lld` in release builds.** The release workflow builds with the same config; it is verified once locally and in
  CI before the commit lands.
- **The 5x may not hold for the matched scenarios** where TL-ADE's test is already small (a pure function in a few
  milliseconds). Each pair's report shows both numbers; a pair under 5x is listed, not hidden.

## 7. Cold runs (R114, Erick)

TL-ADE has no compile step, so a cold run is mostly Faber's handicap: about 3.5 minutes of Rust compilation before the
first test. Only developers meet it (the owner, the AI, self-development); installed apps arrive compiled. The cold
target is 2x, reached mainly through 4.5; warm runs and the matched scenarios keep 5x.

## 8. Order

1. Commit 4.0 after the proof commands pass.
2. The benchmark tool and the first fair report (before numbers).
3. 4.3 waits, then 4.1 shared app, then 4.2 lanes, measuring after each.
4. 4.4 build skip, 4.5 cold cache.
5. The final report in `docs/research/bench/`, and the screen-tests section of `docs/design.md` updated.

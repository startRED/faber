# Test System Rework Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Faber's tests and its real work as fast and light as possible against TL-ADE, measured fairly, with no
test weakened.

**Architecture:** First land the measured build changes (step 0), then a job-object meter that measures both projects the
same way, then cut the screen suite's cost in measured steps (event waits, parallel lanes, build skip), each kept only if
the meter shows a gain. The shared running app and the real-work tier get their own plans once the meter's numbers say
where the cost is.

**Tech Stack:** Rust (testkit, `cargo t`), PowerShell 5.1 with an inline C# helper for the meter (the workspace forbids
`unsafe`, and no installed crate wraps job accounting), SolidJS test build, WebView2 over DevTools.

**Spec:** `docs/superpowers/specs/2026-10-14-test-system-rework-design.md`

## Global Constraints

- R113 floor: at least 5x less wall time and processes started, 3x less peak memory and CPU than TL-ADE, on the warm
  whole suite, each of the 8 matched scenarios and each real-work task.
- R115: compilation is never part of the comparison. R116: no ceiling; a change stays only if the meter shows a gain.
- No test is weakened, shortened or dropped. Waits that prove "nothing happens for N s" and the 10 s idle-memory settle
  keep their real length.
- Same conditions both sides: idle PC, half the logical cores, median of 3 runs, commits and versions recorded.
- AGENTS.md: English, original code (never copy TL-ADE), no visible windows, no credentials, proof commands green before
  every commit, one commit per proven change.
- Rust lives outside the default folders: `export RUSTUP_HOME='E:\Apps\rust\rustup' CARGO_HOME='E:\Apps\rust\cargo'
  PATH="/e/Apps/rust/cargo/bin:$PATH"`.
- Never build in a scratchpad; delete any extra target folder after measuring (disk hygiene).

## Review Focus

- A new engine test file added on another branch (`selfdev*`) must still run: the `every_test_file_is_listed` test
  fails until it is listed in `engine/tests/all.rs`.
- A scenario that fails only when lanes run in parallel must fail the run, never be retried silently.
- The meter must count processes that start and end between samples (the old sampler missed TL-ADE's short `git` runs).
- `rust-lld` must also build the release app (LTO, Windows resources); verify before step 0 lands.
- Another session may be running `cargo` on the PC (`target\t13`): the meter refuses to start while foreign builds run.

## State when this plan was written (2026-10-14)

Measured with the old whole-PC sampler (noisy). Before → after step 0, rust suite: nothing changed 38.2 → 20.9 s;
one engine file changed 62.1 → 27.5 s, peak 7.8 → 1.4 GB, CPU 256 → 86 s; fresh target folder 204 → 200 s, 8.08 →
3.19 GB on disk. Whole suite before: Faber 562 s, TL-ADE 667 s. The "after" whole-suite run was not valid (the debug
change rebuilt the test app) and must be measured again in Task 0.

---

### Task 0: Land step 0 (one test program, rust-lld, debug info, below-normal priority) — done 2026-10-14

**Files:**
- Create: `engine/tests/all.rs`
- Modify: `engine/Cargo.toml`, `Cargo.toml`, `.cargo/config.toml`, `testkit/src/bin/faber-test.rs`

**Interfaces:**
- Produces: test program `engine` (all engine test files) and `install_memory` (alone); test names gain the file as a
  prefix (`account::name`).

- [ ] **Step 1: Apply the changes**

`engine/Cargo.toml`, after `repository.workspace = true`:

```toml
# One test program for every file in tests/ (see tests/all.rs), plus the memory test that must run alone.
autotests = false
```

and at the end:

```toml
[[test]]
name = "engine"
path = "tests/all.rs"

[[test]]
name = "install_memory"
path = "tests/install_memory.rs"
```

`Cargo.toml`, before `[profile.release]`:

```toml
# Line numbers for our own panics, no debug info for dependencies: smaller test programs, faster and lighter links.
[profile.dev]
debug = "line-tables-only"

[profile.dev.package."*"]
debug = false
```

`.cargo/config.toml`, at the end:

```toml
# LLVM's linker ships with the Rust toolchain and links much faster than the Visual Studio one.
[target.x86_64-pc-windows-msvc]
linker = "rust-lld.exe"
```

`testkit/src/bin/faber-test.rs`:

```rust
//! The runner: `cargo t` (see `.cargo/config.toml`). All of it is in `faber_testkit::runner`.

fn main() {
    // Below normal: whatever the person is doing (a game, a call) wins the CPU. Everything the runner starts (cargo,
    // rustc, the linker, the test programs, the hidden app) inherits this class.
    #[cfg(windows)]
    let _ = winsafe::HPROCESS::GetCurrentProcess().SetPriorityClass(winsafe::co::PRIORITY_CLASS::BELOW_NORMAL);
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(faber_testkit::runner::run(&args));
}
```

`engine/tests/all.rs`:

```rust
//! The engine's integration tests, linked into one test program instead of one per file: on Windows each program is a
//! separate link and a separate process, and one changed file relinked all 27 at once (33 linkers, 7.8 GB). The files
//! stay where they are; a new one must be listed here, and `every_test_file_is_listed` fails until it is.
//! `install_memory.rs` keeps its own program: it reads the memory of its own process, which no other test may share.

#[path = "account.rs"]
mod account;
#[path = "auto.rs"]
mod auto;
#[path = "budget.rs"]
mod budget;
#[path = "cache_health.rs"]
mod cache_health;
#[path = "claude_session.rs"]
mod claude_session;
#[path = "codex_session.rs"]
mod codex_session;
#[path = "engine.rs"]
mod engine;
#[path = "handover.rs"]
mod handover;
#[path = "install.rs"]
mod install;
#[path = "learn.rs"]
mod learn;
#[path = "learning.rs"]
mod learning;
#[path = "ledger.rs"]
mod ledger;
#[path = "pacing.rs"]
mod pacing;
#[path = "paid_guard.rs"]
mod paid_guard;
#[path = "plan.rs"]
mod plan;
#[path = "project_security.rs"]
mod project_security;
#[path = "quota.rs"]
mod quota;
#[path = "route.rs"]
mod route;
#[path = "route_pacing.rs"]
mod route_pacing;
#[path = "running.rs"]
mod running;
#[path = "signals.rs"]
mod signals;
#[path = "simulation.rs"]
mod simulation;
#[path = "turn_usage.rs"]
mod turn_usage;
#[path = "usage_view.rs"]
mod usage_view;
#[path = "waiting.rs"]
mod waiting;
#[path = "workspace.rs"]
mod workspace;

#[test]
fn every_test_file_is_listed() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let this = include_str!("all.rs");
    let mut missing = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        let alone = ["all.rs", "install_memory.rs"].contains(&name.as_str());
        if name.ends_with(".rs") && !alone && !this.contains(&format!("#[path = \"{name}\"]")) {
            missing.push(name);
        }
    }
    assert!(missing.is_empty(), "add these to engine/tests/all.rs: {missing:?}");
}
```

If the branch has gained engine test files since 2026-10-14, list them too (the guard test names them).

`all.rs` also declares `mod loopback;` and `mod support;` once, before the file list, and every test file replaces its own
`mod support;` (or `mod loopback;` in `install.rs`) with `use crate::support;` (`use crate::loopback;`): Clippy's
`duplicate_mod` refuses a module loaded once per file. `install_memory.rs` keeps its own `mod loopback;`.

- [ ] **Step 2: Prove the guard fails, then passes**

Create an empty `engine/tests/zz_probe.rs`, run `cargo test -p faber-engine --test engine every_test_file_is_listed`.
Expected: FAIL naming `zz_probe.rs`. Delete the file and run again. Expected: PASS.

- [ ] **Step 3: Verify the release build links with rust-lld**

Run: `cargo build -p faber --release` (long; LTO). Expected: exit 0, and `target/release/faber.exe` starts and shows its
icon (Windows resources linked). If it fails, scope the linker to the test runner instead: remove the
`[target.x86_64-pc-windows-msvc]` block and set `CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER=rust-lld.exe` on the
`cargo test` command in `testkit/src/runner.rs:run_rust` and in `testbuild.rs:ensure_fake_cli`.

- [ ] **Step 4: Run every proof command**

```bash
npm run lint && npm run typecheck && npm run build && cargo fmt --all --check \
  && cargo clippy --workspace --all-targets -- -D warnings \
  && cargo clippy -p faber --features test-hooks -- -D warnings && cargo t --all
```

Expected: all exit 0.

- [ ] **Step 5: Commit and push**

```bash
git add engine/tests/all.rs engine/Cargo.toml Cargo.toml .cargo/config.toml testkit/src/bin/faber-test.rs
git commit -m "build(test): one engine test program, rust-lld, lighter debug info, runner below normal priority"
git push
```

### Task 1: The meter (`scripts/bench/measure.ps1`)

**Files:**
- Create: `scripts/bench/measure.ps1`, `scripts/bench/JobMeter.cs`
- Test: `scripts/bench/measure.check.ps1`

**Interfaces:**
- Produces: `measure.ps1 -Label <s> -Dir <path> -Exe <path> -ArgLine <s> -Out <jsonl>` appends one JSON line:
  `{label, exit, wall_s, procs_started, cpu_s, peak_commit_mb, written_mb, repo_commit, date}`.

- [ ] **Step 1: Write the C# helper**

`scripts/bench/JobMeter.cs`: a static class loaded with `Add-Type -Path`. `Run(string exe, string args, string dir)`
creates a job object (`CreateJobObject`), starts the process with `CreateProcess` and `CREATE_SUSPENDED |
CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS`, assigns it (`AssignProcessToJobObject`), resumes its thread
(`ResumeThread`), waits for the process, then waits until the job's `ActiveProcesses` is 0 (descendants that outlive the
root still count), and reads `QueryInformationJobObject` with `JobObjectBasicAccountingInformation` (`TotalProcesses`,
`TotalUserTime`, `TotalKernelTime`), `JobObjectExtendedLimitInformation` (`PeakJobMemoryUsed`) and
`JobObjectBasicAndIoAccountingInformation` (`WriteTransferCount`). It returns those numbers, the exit code and the wall
time. Standard output and error go to files next to the JSON line.

- [ ] **Step 2: Write the conditions into `measure.ps1`**

Before starting: refuse while another `cargo.exe`, `rustc.exe` or `vitest` runs outside this job (another session's
build); wait until `GetLastInputInfo` shows no input for 60 s; refuse while a full-screen app runs
(`SHQueryUserNotificationState` not `QUNS_ACCEPTS_NOTIFICATIONS`). Set `CARGO_BUILD_JOBS` and TL-ADE's worker cap to half
the logical cores. Record `git rev-parse HEAD` of `-Dir` and the date.

- [ ] **Step 3: Write the check**

`measure.check.ps1` measures `faber-test`'s helper `fake-cli --family` (it starts two `--idle` copies of itself, which
end after a minute): expected `procs_started` = 3, `wall_s` ≥ 60, `exit` = 0. Run it; it must fail before Step 1 exists
and pass after.

- [ ] **Step 4: Commit**

```bash
git add scripts/bench
git commit -m "test(bench): a job-object meter that measures Faber and TL-ADE the same way"
```

### Task 2: The fair report

**Files:**
- Create: `scripts/bench/run.ps1`, `scripts/bench/matched.json`, `docs/research/bench/README.md`
- Output: `docs/research/bench/2026-10-xx.jsonl`

**Interfaces:**
- Consumes: `measure.ps1` from Task 1.
- Produces: `run.ps1 -Tag <s> [-Only suites|matched]` running each measurement 3 times and writing the medians.

- [ ] **Step 1: List the matched scenarios**

`matched.json`, one entry per spec section 3 row: `{ "id": 1, "behaviour": "...", "tlade": ["tests/adapter-claude.test.ts",
"tests/fake-cli.test.ts"], "faber": "claude_session::" }`. TL-ADE runs `node node_modules/vitest/vitest.mjs run <files>`
in its folder; Faber runs `cargo t --suite rust --filter <faber>`. Use the file names from the spec table; for each
Faber filter, check with `cargo t --suite rust --filter <x>` that it selects only that scenario's tests.

- [ ] **Step 2: Measure the warm suites and the matched scenarios**

`run.ps1` warms each side once (not recorded), then records 3 runs each of: TL-ADE `vitest run`; Faber `cargo t --all`;
each matched scenario on both sides. It writes per-row medians and the TL-ADE/Faber ratio for each metric.

- [ ] **Step 3: Write `docs/research/bench/README.md`**

How to run it, the conditions, and the first table: whole suite and each matched scenario, with the five metrics and the
ratios. Mark every row under the R113 floor.

- [ ] **Step 4: Commit**

```bash
git add scripts/bench docs/research/bench
git commit -m "docs(bench): first fair report of Faber against TL-ADE"
```

### Task 3: Waits follow events

**Files:**
- Modify: `testkit/src/screen/mod.rs` (add `settle`), `testkit/src/page.rs` (if a page helper is needed),
  `testkit/src/screen/{sentinel,shell,first_run,setup,app,review,settings,permissions}.rs`, `testkit/src/app.rs:539-593`

**Interfaces:**
- Produces: `pub fn settle(page: &mut Page, selector: &str) -> Result` — returns when the element exists, has no running
  animations (`el.getAnimations({subtree:true}).every(a => a.playState !== 'running')`) and two animation frames passed;
  times out after 5 s with the selector in the message.

- [ ] **Step 1: Write the failing check for `settle`**

In `testkit/src/screen/sentinel.rs`, add to an existing scenario that opens a panel: after the click, `settle(page,
".panel")?` then assert the panel's `getBoundingClientRect()` equals the value read 300 ms later (it is still). Run
`cargo t --suite screen --filter sentinel`. Expected: FAIL (`settle` not defined).

- [ ] **Step 2: Implement `settle`**

```rust
/// Waits until `selector` exists and has stopped moving: no running animation in it, then two frames.
pub fn settle(page: &mut Page, selector: &str) -> Result {
    let still = format!(
        "(() => {{ const el = document.querySelector({sel}); return !!el && el.getAnimations({{subtree: true}}).every(a => a.playState !== 'running'); }})()",
        sel = serde_json::to_string(selector).unwrap()
    );
    page.wait_for_within(&format!("{selector} to settle"), &still, Duration::from_secs(5))?;
    page.eval("new Promise(r => requestAnimationFrame(() => requestAnimationFrame(() => r(true))))").map(|_| ())
}
```

Check `Page::wait_for_within` and `Page::eval` names in `testkit/src/page.rs` and use the existing ones.

- [ ] **Step 3: Replace the pauses that wait for motion**

For each of the 63 `pause(` calls: if it waits for an animation, a transition or a debounce to finish before reading the
page, replace it with `settle(page, "<the element read next>")?`. Keep, unchanged, every pause that proves that nothing
happens for a time (`first_run.rs:614`, `shell.rs:351`, `shell.rs:387`, `shell.rs:927`, `setup.rs:231`, `setup.rs:316`,
`first_run.rs:654`) and the idle-memory settles (`app.rs:636`, `app.rs:651`, `app.rs:676`, `app.rs:717`). Leave a
one-line comment on each kept pause saying what it proves.

- [ ] **Step 4: Teardown waits on the job**

In `app.rs` `finish` and `cleanup`, replace the 100 ms polling of `survivors()` with a loop on `Owned::members()`
(`contain.rs:136`) that returns as soon as the job lists no process, then runs `survivors()` once for the audit.

- [ ] **Step 5: Measure, then keep or drop**

Run `cargo t --suite screen` 3 times; the screen suite total must not grow, every scenario must pass 3 times in a row,
and `run.ps1 -Only suites` must show a gain. Commit only then:

```bash
git commit -am "test(screen): wait for motion to settle instead of fixed pauses"
```

### Task 4: Parallel lanes

**Files:**
- Modify: `testkit/src/screen/mod.rs` (`Scenario`, `run_all`), `testkit/src/app.rs` (`free_port`, `Ctx::launch`),
  every `scenarios()` list
- Test: `testkit/src/screen/mod.rs` tests module

**Interfaces:**
- Produces: `pub struct Scenario { pub name: &'static str, pub run: fn(&mut Ctx) -> Result, pub alone: bool }`;
  `pub fn lanes(cores: usize) -> usize` (1 below 8 logical cores, else 3); ports from `PortRange::for_lane(lane)`.

- [ ] **Step 1: Failing tests**

```rust
#[test]
fn lanes_follow_the_core_count() {
    assert_eq!(lanes(4), 1);
    assert_eq!(lanes(8), 3);
    assert_eq!(lanes(32), 3);
}

#[test]
fn memory_timing_and_fresh_start_scenarios_run_alone() {
    for s in all() {
        let must = ["memory", "first run", "setup", "restart", "reopen", "containment", "hidden desktop", "within", "frame budget"]
            .iter()
            .any(|w| s.name.contains(w));
        assert!(!must || s.alone, "{} must run alone", s.name);
    }
}
```

Run `cargo test -p faber-testkit lanes_follow memory_timing`. Expected: FAIL.

- [ ] **Step 2: Implement**

Add `alone` to every scenario (true for the names above and any scenario that measures memory or time); `lanes()`;
`run_all` runs non-alone scenarios on `lanes()` threads (each thread takes the next scenario from a shared queue and
runs `run_one`), then the alone ones one by one; outcomes print in the order they finish and the final list is sorted
by the original order. Replace `free_port()` with ports handed out by a per-run counter starting at a random base in
20000..40000, checked free by binding, never released and rebound by another lane.

- [ ] **Step 3: Prove isolation**

Add a scenario pair that runs in two lanes at once: one writes a marker file into its sandbox and leaves a fake CLI
running; the other asserts it sees neither. Expected: both pass, and the first one's audit fails on the leaked CLI
(prove by running it alone with the leak kept).

- [ ] **Step 4: Measure, then keep or drop**

`cargo t --suite screen` 3 times in a row with every scenario passing; `run.ps1 -Only suites` shows a gain. Commit:

```bash
git commit -am "test(screen): run independent scenarios on parallel lanes"
```

### Task 5: Skip Vite when the screen did not change

**Files:**
- Modify: `testkit/src/testbuild.rs:84-98`
- Test: `testkit/src/testbuild.rs` tests module

**Interfaces:**
- Consumes: `fingerprint(&Path) -> String` (`testbuild.rs`).
- Produces: `fn ui_inputs(root: &Path) -> String` — the fingerprint of `ui/`, `tests/ui/`, `index.html`,
  `vite.config.*`, `package.json`; stored in `dist-test/.inputs`.

- [ ] **Step 1: Failing test**

```rust
#[test]
fn vite_runs_only_when_the_screen_inputs_changed() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("ui")).unwrap();
    std::fs::write(dir.path().join("ui/a.ts"), "1").unwrap();
    std::fs::create_dir_all(dir.path().join("dist-test")).unwrap();
    assert!(needs_vite(dir.path()), "no stamp yet");
    std::fs::write(dir.path().join("dist-test/.inputs"), ui_inputs(dir.path())).unwrap();
    assert!(!needs_vite(dir.path()));
    std::fs::write(dir.path().join("ui/a.ts"), "2").unwrap();
    assert!(needs_vite(dir.path()));
}
```

- [ ] **Step 2: Implement `needs_vite` and write the stamp after a successful Vite build**

`build_test_app` runs Vite only when `needs_vite(&paths.root)`, then writes `dist-test/.inputs`. Tauri still runs (it is
incremental).

- [ ] **Step 3: Measure, then commit**

```bash
git commit -am "test(build): skip the Vite test build when the screen did not change"
```

### Task 6: Shared running app (own plan)

Write `docs/superpowers/plans/2026-10-xx-shared-test-app.md` after Task 5, from the report's numbers. It covers spec
section 4.1: the `alone` flag from Task 4 marks the fresh-start scenarios; a test-hooks-only `test_reset` command in
`src-tauri` (ends the engine's CLIs, drops its in-memory state, switches it to a new data folder) with a page reload; the
runner's proof after each reset (job holds only the app, old sandbox clean, new data folder empty, fresh screen shown);
and a test that plants leftover state and expects the proof to fail. It must start by reading how `Launch`,
`child_env` (`testkit/src/sandbox.rs:193`) and `write_settings` give each scenario its own environment today, because a
shared app cannot change its environment after launch.

### Task 7: Real-work tier (own plan)

Write `docs/superpowers/plans/2026-10-xx-real-work-bench.md`: the 7 tasks of spec section 3.1, scripted once per app over
DevTools on the hidden desktop, inside the meter. It starts by finding how TL-ADE starts its engine and panel with a fake
CLI (read its docs and `bin/`, never copy its code), and which tasks it cannot do.

### Task 8: Final report

- [ ] Run `run.ps1` with every tier; write the table into `docs/research/bench/README.md`.
- [ ] Update the "Screen tests" section of `docs/design.md` (lanes, settle, shared app if Task 6 landed).
- [ ] If a row is still under the R113 floor, the next round starts on the largest cost in the report (R116).
- [ ] Commit and push.

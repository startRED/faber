# Testing

Faber's own test system: `cargo t`, screen tests, the build folder and what a run prints.

The runner resolves the repository from its current folder or the nearest ancestor containing `Cargo.toml`,
`testkit/Cargo.toml` and `engine/Cargo.toml`. Run it inside the intended checkout, even when reusing a warmed
`CARGO_TARGET_DIR`; outside a Faber workspace it fails instead of using the checkout where it was compiled.
The fake CLI stays beside the runner executable, and the configured target folder is preserved.

`Launch::claude_recording` and `codex_recording` can select independent fake replays for a main agent and its helper.
They add `--faber-recording <path>` to that fake binary's structured argument prefix; the fake removes it before
handling status, setup or session arguments. Unset overrides keep the shared `Launch::recording` fallback.
These options change only the test harness: the sandbox, hidden desktop and real CLI isolation stay the same.

## Screen tests

Faber's own test runner (`cargo t`, the `testkit/` crate) tests the screen in two ways, both against a hidden test build
of the app. `FABER_HIDDEN=1` keeps the window offscreen with its own browser profile, so it never takes the user's screen.

- **Checks** (`tests/ui/*.check.ts`) test the screen's own logic. The test build opens `index.html?checks`, runs them when
  the page loads, writes the result to the file named by `FABER_CHECKS_OUT` and exits. No DevTools port is involved, so
  they run on Windows, macOS and Linux (Linux under `xvfb-run -a`).
  With `--filter`, only check bodies whose names contain that text run; startup and module registration remain shared.
  Checks start after the shared look and context menu bootstrap; the normal App is not mounted beside their own App
  fixtures, so startup cannot change their language or focus. Test builds without `?checks` still open the normal app.
- **Scenarios** (`testkit/src/screen/`, written in Rust) wait for the rendered app and its text/sentinel helpers within the existing startup or reload deadline, then drive it like a person: they read the page, click and
  type, and save screenshots (named checkpoints, and every failure) to `target/faber-test/shots/`. On Windows the UI
  runs in WebView2, which exposes Chromium's DevTools port (`FABER_CDP_PORT`); the macOS and Linux webviews have no such
  port, so those systems skip this suite and run every other check.

**The window never reaches the person's desktop (Windows).** `FABER_HIDDEN=1` alone is not enough: an offscreen window can
still become visible and take focus, which matters to someone playing full screen. So the runner (`testkit/src/desktop.rs`)
makes a separate Windows desktop once per run (`CreateDesktopW`, named `faber-test-<pid>-...`), which no monitor shows and
no input reaches, and every app launch starts on it (`STARTUPINFO.lpDesktop`, through the gate helper, still inside its
job object and with no console window). The app's window and its WebView2 processes inherit that desktop, so they cannot
be seen or steal focus; the DevTools port is a TCP port and does not care which desktop the page lives on. Two safety
nets: if the desktop cannot be made or assigned, nothing is launched and the run fails (there is no fallback to the
normal desktop); and after launch, and at every audit point, the runner lists the visible top-level windows on the
desktop in use and, if one belongs to the app's job, ends the job at once and fails. The scenario `hidden desktop:
...` proves the mechanism without Faber: a helper started like the app lists the windows of its own desktop, and none of
them is a window of the desktop in use.

**macOS keeps the page awake.** A window placed at (-32000, -32000) counts as occluded there, and WebKit froze the checks
page's whole event loop partway through a run on CI. So the macOS test build (`test-hooks`, hidden) sits at the screen's
corner under every other window, ignores the mouse, never takes focus, and sets WebKit's inactive scheduling policy to
`Disabled` (`background_throttling`, macOS 14+). Windows keeps the offscreen place and its own desktop.

The four audited ancestor folders use compact names because WebView2 adds nested database paths below the private build
temporary folder. Longer ancestor names reproduced an IndexedDB backing-store open failure in a long Windows cache path;
shortening their names fixed that fixture without dropping any audit level or moving data outside the sandbox.

Every launch is sandboxed: a temporary home, an empty `PATH`, no provider variables, a dead proxy and no real CLI (a fake
CLI replays the recordings in `tests/fixtures/`). Each scenario is audited afterwards for leaked processes, writes outside
the sandbox and a changed environment. What the app started must end by itself once the app is ended, except WebView2's
own processes: the runtime shuts its browser down in its own time, which on a loaded PC or runner took longer than the
audit's 15 s, so the cleanup ends that tree through the app's job and waits until the system says the job is empty. On Windows the runner lists processes (pid, parent, start time, processor time,
private memory) with one `NtQuerySystemInformation` call (`faber_winproc::processes`, the one crate allowed unsafe code),
which takes a few milliseconds; the WMI query it replaced took 150-200 ms, and a scenario takes 9 to 49 of these lists.
The tree is still followed by parent and start time, so a pid the system reused is never mistaken for one of the app's.
After a visual-frame collection or post-close memory assertion fails, the runner records owned process memory,
aggregate page heap and process roles from that fixture's own loopback browser. Browser queries have separate
three-second deadlines; their errors and capture delay remain evidence, never a substitute for the original failure.
These observations follow the captured assertion values and cannot establish the exact deadline state or the cause
of an earlier failure. Process enumeration may also delay reporting; no memory limit or collection step changes.
A page wait (`Cdp::settle`) runs inside the page: one DevTools call of up to two seconds reads the condition again at
every change of the document (at most every 16 ms, and every 50 ms without a change), instead of one call every 100 ms
from the runner. A condition the page cannot wrap, statements rather than one expression, is asked every 100 ms as before. The checks and the hooks that report them exist only in the test build
(`--features test-hooks`, `vite build --mode test`); the production app and its `dist/` contain none of them. CI runs
only the jobs a change needs (R163). A first job reads the changed files and picks the suites by the same rules as
`cargo t` (`testkit/suites.json`, read by `testkit/src/plan.rs` and `scripts/ci-scope.mjs`): words only (`docs/` or a
`.md` file) need none, `docs/product-map.md` needs the `rust` suite because a test reads it, the test files of the engine
and the shell need only the `rust` suite, a manifest, the runner, CI or a file no rule knows needs every suite, and any
file that is not words runs the lints. A push is compared with the commit before it, and a queue commit with
`phase-1-base`, where it lands; either may skip a suite only when the commit it builds on is green, and anything the job
cannot tell runs everything (R165). A queue commit whose very tree carries the PC's green `faber/verify` leaves out
the suites it covers, which `land.mjs` ran on this PC before queueing ([workflow](workflow.md#merge-on-green), R190 V2).
Every suite still runs once a day on `main`, and on a commit before it becomes
stable (`promote-stable.yml` starts the run and waits for it). The lints and the `rust` suite run on Windows and on macOS
(macOS adds the `ui` suite to its lints). One Windows job builds the test app once (`cargo t --build-only`, the fake
CLI and the cache example at the same time in Cargo's own folder) and hands its programs (`built_programs` in
`testkit/src/testbuild.rs`) to a `ui` part and eight `screen` parts, which build nothing
(`--suite screen --shard 2/8 --prebuilt`) and use the Rust the runner image has. On CI the runner uses every core; on a
person's PC it keeps to half, and every build it starts there (Cargo, Vite, Tauri) runs at idle priority with no
console window, so a game in front keeps the processor. A local run builds the test screen with Vite while Cargo builds
the fake CLI, then the shell; Vite is skipped when `dist-test` already holds the build of the same inputs (by
content: the screen, its checks, the shell file they import, and the build's configuration and packages; a test fails
when an import reaches a file outside that list). Every Cargo the runner starts goes without the variables `cargo run` gave it, which made
ring's build script, and everything above it, build again. Each kind of job keeps its own Rust cache, and only `main`
saves it: a run can read only the caches of its own branch and of `main`, and branches that each saved their own pushed
the others out of the 10 GB GitHub keeps. Since pushes made by CI start no run, the `land` and `main` jobs start a warm run
on `main` after it moves, which builds what each kind of job builds and saves it when the dependencies changed. The runner
deals the scenarios so the parts take about the same time: the slowest first, each to the lightest part so far, by the CI
seconds of each scenario. A `costs` job reads them from the logs of the newest green runs (`scripts/screen-costs.mjs`) and
hands every part the same table (`--costs`), so the dealing never ages; without those logs, and on a person's PC, the copy
in `testkit/screen-costs.json` counts, and a scenario missing from it counts as 5 s. CI keeps the Windows screenshots as artifacts.
Anything outside the page (tray, notifications, installer, updates) is checked through the operating system's accessibility layer and file versions.

Test launches also pass the browser engine switches that turn off crash reports and SmartScreen address lookups, and the
sandbox removes every `WEBVIEW2_*` override from the environment. That limits what a test sends; it is not a firewall.
Nothing blocks the network at the operating system, and WebView2 has no documented switch for crash uploads, so a request
that bypasses the switches is not seen. The audit sees only requests that reach the fake release server.
Known limit: the loopback-only rule is not enforced. Test processes get dead proxy variables and the switches above, but
nothing stops a program from opening its own connection; blocking outbound traffic for test processes is deferred.

### Parallel scenarios

The pane-memory scenario always measures offline HTML. Its real Mermaid CDN renderer and cost step runs only with
`FABER_LIVE_CDN=1`; otherwise the log says Mermaid Live was not measured. That opt-in lead run requires internet and
fails if the pinned library cannot draw; ordinary screen suites do not depend on CDN availability.

On a person's PC the screen suite runs several scenarios at once, so a full run takes a fraction of the time it took one
after the other (1432 s before, 497 s after with four workers, on the maintainer's PC, 2026-10-17). `FABER_SCREEN_WORKERS` sets how many
(a number above zero); without it a PC uses 4 workers and GitHub Actions uses 1, since each CI part already shares a small
machine. Each worker takes the next scenario when it is free, the slowest first (by `testkit/screen-costs.json`), and
each scenario has what it had before: its own sandbox, its own DevTools port (the runner never hands one port to two
scenarios), its own job object and process tree (the audit looks only at the processes its scenario recorded), and a
fake release server of its own. `--shard` still picks the scenarios of a part; the workers run what it picked. A
screenshot is written beside its name and renamed into place, so two scenarios that name one checkpoint never mix them. The
outcomes print as they end, but the list and the summary keep the order of the scenarios.

Scenarios whose result depends on the machine being quiet run alone, one at a time, after the parallel batch, with
nothing else running (`SOLO` in `testkit/src/screen/mod.rs`, each with its reason). They are the ones that read the
processor or memory of the app, count frames or time a motion, depend on which window is shown or has the focus, wait
for the quota (a background reading at idle priority, which a busy processor starves) or use the PC's one clipboard. When
unsure, a scenario goes in that list. A test fails when a scenario whose name says "memory", "processor", "CPU", "MB",
"budget", "frame", "focused", "quota" or "usage limits" is not in it, and when a name in the list is no scenario any more.
A scenario that fails only in parallel is added to the list, or its race is fixed.

### Build folder budget

[Build folders](build-folders.md) describes the shared and temporary build roots, cleanup rules and how to preserve
exact proof artifacts outside disposable caches.

### Build slots

Many lanes run `cargo t` on one PC; each run peaks near 8 GB. On 2026-10-16 and 17 lanes spent 441 minutes waiting for a
slot, so the suites now run in CI on the queued commit and a lane runs `cargo t --claims` before a commit, which builds
only the runner, takes no slot and checks the claims and the red base (R183). Before building, a run takes one of three slots
(`testkit/src/slots.rs`): a locked file `target/.faber-slots/slot-<n>` of the main checkout, held until the run ends,
crash included. With all three taken it prints one waiting line and waits. `FABER_BUILD_SLOTS` sets another count for
another PC (R129, E3). Idle priority gives the processor away but not the memory, so before its slot a run also waits
while a full-screen app is in front (a game, a presentation; a borderless window is not seen) or while less memory is free
than one run takes (8 GB), at most ten minutes for memory, with one `Waiting:` line per reason (R440). On 2026-10-22 three runs left 2.2 GB of 36 free
under a game. CI never waits.

### What a run prints

A run prints only what needs a look (R154): each failure with its reason, each pass that measures memory or processor
(R125), and one summary line. A failed Rust test shows its panic from the `file:line` on; a compiler error shows as
`file:line:col: error[code]: message`, with the compiler's suggested fix as one `help:` line under it. A reason longer
than 15 lines keeps its first line and its error lines. Every line, every pass and Cargo's whole output included, goes
to `target/faber-test/run.log`, which CI keeps with the screenshots. Cargo's output is read as it comes, so a long
build shows in the log while it runs. The run also times itself: the summary ends with one `Time:` line (the whole run,
then build, lint, rust, ui and screen), `run.log` gets a `time` line per scenario with its steps (launch, process lists,
page waits, pauses, audit, cleanup) and one line with the totals of every step, and `target/faber-test/timing.json`
keeps the same numbers for a script (`testkit/src/timing.rs`). `cargo test` runs with `--no-fail-fast`, so one run
lists every failing test. `cargo t --suite lint` runs the two Clippy proofs the same way, and a test keeps them equal to
CI's lines. The `cargo t` alias builds the runner itself with `--message-format=short`, so a broken build of the runner
also prints one line per error. `cargo t --failed` runs again only what failed in the last run, read from the `FAIL` lines of
`run.log` (screen scenarios and Rust tests by exact name, the `ui` and `lint` suites whole); with none recorded it says so and runs nothing.

On Windows the lint suite adds a third proof: Clippy for macOS (`aarch64-apple-darwin`) on the engine and the runner, so
code behind `cfg(unix)` or `cfg(target_os = "macos")` breaks before the push, not a CI round later (R166,
`testkit/src/cross.rs`). Clippy links nothing, but `ring` (TLS crypto) compiles C, so zig stands in for the Mac's C compiler:
Cargo calls the runner as `CC` and `AR` and the runner hands the call to zig. It needs zig on `PATH` and
`rustup target add aarch64-apple-darwin`; without zig the proof is skipped and says so. The shell is left out, since its
Objective-C needs Apple's SDK; CI's Mac still checks everything.

A mission reads its checks the same way (R155, `engine/src/project/proof/output.rs`, which `cargo t` also uses for
Cargo). The roles get each failure with its `file:line` and the tool's fix, the measurements and one summary line, at
most 20 failures of 8 lines; the whole output stays in the mission's private `proofs/` folder. It reads `cargo t`,
rustc and Clippy, `cargo test`, `cargo fmt --check`, tsc and oxlint; any other format gives its last 20 lines when the
check fails. A new format is one more parser in that file's list. Lines that look like a secret are withheld first.

## Faber's own test system

One Rust program replaces the whole Node test suite. The reason is to avoid the CPU and RAM load that a heavy test
suite causes. Node stays a development tool for Vite, `tsc` and `oxlint`; it never runs tests and never ships.
The program is `faber-test`, run as `cargo t`.

1. **One light program.** It compiles with half the cores. Locally it runs only the tests that depend on the files a
   change touched. The full set runs on GitHub, on Windows, macOS and Linux.
2. **Fake CLIs that replay recorded conversations.** No test calls a real model or spends quota, and each runs in
   milliseconds. Changing one field of a recording must turn its test red.
3. **Screen tests run inside a hidden Faber**, opened only when needed. On Windows they drive the real app through the
   WebView2 DevTools port; the checks that need no such port run on every system.
4. **Unit tests never spawn processes, touch real git or call a real CLI.**
5. **The benchmark of ~40 real requests** that measures the goals against plain Claude Code and plain Codex before
   each version. Its light version (5 requests, `cargo run -p faber-engine --example bench`) is compiled by `cargo t` and the lint suite but never run by them or by CI, since it spends real quota ([benchmark](benchmark.md)).
6. **Faster and lighter than TL-ADE's suite, measured fairly** (R113, R115, R116, owner;
   [spec](../superpowers/specs/2026-10-14-test-system-rework-design.md),
   [plan](../superpowers/plans/2026-10-14-test-system-rework.md)). The floor is 5x less time and processes and 3x less
   memory and CPU, with no ceiling; no test is weakened. Landed: one engine test program, the `rust-lld` linker, lighter
   debug info and below-normal priority (2026-10-14), then in-page waits, parallel scenarios and skipping an unchanged
   screen build (R189); the shared running app was measured and shelved (R395).
   How it is measured: [the fair benchmark](fair-bench.md).
7. **The build folder has a budget** (R106). Every worktree builds into a folder of the main checkout's `target`, and
   Cargo never deletes anything there, so it grew to 123 GB and once filled the disk. `cargo t` sweeps it at start, at
   most once an hour: folders of removed worktrees and builds replaced more than a day ago go, and above 60 GB
   the least recently used worktree folders go too. A folder in use is never touched
   ([design](#build-folder-budget)).

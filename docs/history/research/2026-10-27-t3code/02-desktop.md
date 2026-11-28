# T3 Code desktop shell: study for Faber

Read-only study of T3 Code (MIT) at `t3code/`. Nothing was built, installed or run there. Paths below are relative to that clone.

## Scope and gaps

Read: `apps/desktop/src/main.ts`, `backend/DesktopBackendManager.ts` (key ranges), `updates/DesktopUpdates.ts` and `updates/updateChannels.ts` (key ranges), `electron/ElectronUpdater.ts`, `window/DesktopWindow.ts` (window creation and close, lines 370-400 and 640-700), `window/QuitHold.ts`, `app/DesktopStartupRecovery.ts` (full), `app/DesktopClerk.ts` (single-instance section), `electron/WindowsForeground.ts` and `WindowsForegroundFocusWorker.ts` (headers), `telemetry/DesktopTelemetryPublisher.ts` and `DesktopRendererHistory.ts` (cadence and throttle), `native/resource-monitor` (main.rs constants and loop), `native/browser-secret/main.c` (header), `scripts/install.ps1`, `scripts/verify-windows-desktop-artifact.ts`, `scripts/smoke-desktop-app.ts`, `docs/internals/resource-telemetry.md`, `docs/internals/server-updates.md`, `docs/user/background-service.md`, `docs/user/updating.md` (headings).

Not read in depth: `native/hyprland-snap-shot` and `native/kde-snap-shot` sources (Linux only, low value for Faber), `scripts/release-smoke.ts` (445 lines, skimmed by name only), `docs/user/devices.md`, `snap-shot.md`, `default-browser.md`, `browser-import.md` (headings only), `DesktopObservability.ts`, `DesktopLinuxUrlHandler.ts`, `DesktopAppActivation.ts`.

Faber context used for "already have" checks: `engine/src/proc.rs`, `winproc/src/lib.rs`, `src-tauri/src/local_update.rs`, `src-tauri/src/main.rs`, `src-tauri/tauri.conf.json`.

Key facts about T3 that change how to read it:

- T3 has **no system tray** (no `Tray` usage in `apps/desktop/src`). Faber's tray has no T3 precedent.
- T3's desktop is Electron. Most of its window and IPC code does not port to Tauri 2. The portable parts are the policies, state machines and pure functions, and the measurement approach.
- T3 does not support a Windows background service (`docs/user/background-service.md`, "Windows background services are not supported").

## Summary

| # | Idea | Effort | Kind | Faber today |
|---|------|--------|------|-------------|
| 1 | Adaptive sampling cadence by power, lock and thermal state | S | idea + pure function | no process-memory sampler found |
| 2 | Sampler off unless someone subscribes; bounded history | M | idea | none |
| 3 | Startup recovery window with JavaScript disabled | S/M | idea + pure function | not found |
| 4 | Packaged-app launch smoke test with a seeded profile | S | idea | screen tests are dev-build only |
| 5 | Update channel chosen by version pattern; mismatches ignored | S | pure function | one `latest.json` endpoint |
| 6 | Install record and restart marker before the installer runs | M | idea | probation/rollback exists for local builds only |
| 7 | Renderer memory sampled at a 30 s floor, guarded by PID identity | S | idea | none |
| 8 | Foreground activation with thread-input attach, as a pure function | S | pure function | not found |
| 9 | Hidden-first window, revealed after first paint | S | idea | not checked in detail |
| 10 | Single-instance lock that queues second-instance arguments | S | idea | not checked |
| 11 | Installer verifies SHA256 before extraction; staging dir, then atomic move | S | idea | not checked |
| 12 | Native load probe for the packaged Windows binary on the target arch | S | idea | not found |

Do **not** copy: T3's Windows tree kill (section "What not to copy").

---

## 1. Adaptive sampling cadence by power, lock and thermal state

**What T3 does.** `apps/desktop/src/telemetry/DesktopTelemetryPublisher.ts` lines 28-32 set the intervals: 1 s live, 5 s on battery, 15 s when thermal state is serious or critical, 30 s active and 2 min idle by default. `sampleInterval` (lines ~122-141) is a pure function of power state (suspended, locked, idle, thermal, on battery) and a "diagnostics demand" flag. Without demand, it returns the idle interval whenever the machine is suspended, locked or idle. Host power intervals can be changed at runtime through a message (`setHostPowerIntervals`, ~line 342).

**Why it helps Faber.** Faber's budget is a memory and processor budget with no CLI running (under 500 MB). Any sampler or poll that keeps running while the screen is locked or the machine is on battery costs real CPU and wakeups. A single pure function that maps state to a cadence is cheap to test and to measure.

**Effort:** S.

**Kind:** Idea, plus a small pure function that can be re-implemented in Rust with the same table. Credit T3 (MIT) in the doc comment if the table is copied.

**Faber status:** The engine idle timers (`src-tauri/src/main.rs` ~1553, 1739: `idle: Duration::from_secs(120)`) and `awake.rs` (~342) exist. No adaptive cadence for locked, suspended or thermal states was found.

**Measure it:** count wakeups per minute and CPU time in idle with the screen locked, before and after.

---

## 2. Sampler off unless someone subscribes; bounded history

**What T3 does.** `native/resource-monitor/src/main.rs`: `SetSampleInterval` with 0 means no timer at all. The main loop (lines ~783-960) waits on `recv_timeout` until the next deadline, and falls back to 60 s when nothing is scheduled. On-demand `SampleNow` does not move the periodic deadline. Constants at lines 13-24: `MIN_SAMPLE_INTERVAL_MS` 250, `MAX` 60000, `HISTORY_RETENTION_MS` 1 h, `MAX_HISTORY_SNAPSHOTS` 3600, `MAX_HISTORY_RETAINED_ENTRIES` 20000, `MAX_HISTORY_RETAINED_BYTES` 64 MiB, plus byte caps on process name, command and status. `docs/internals/resource-telemetry.md` states the rule: the sampler is idle with no subscribers, and history has four independent bounds (age, count, rows, bytes).

**Why it helps Faber.** "Idle until asked" is the cheapest possible default. Four independent bounds on any history stop a long-running app from growing its memory, which is exactly what the R125 limit guards against.

**Effort:** M (the sidecar is a separate process; the pattern is what ports).

**Kind:** Idea. The sidecar itself should not be added to Faber unless a measurement shows it is needed. Faber's own rule is that a process kept alive needs a reason and a measurement.

**Measure it:** T3's doc gives a measurement trap list that applies directly: PID reuse needs start-time identity, snapshot sequence per monitor generation, and Windows I/O semantics.

---

## 3. Startup recovery window with JavaScript disabled

**What T3 does.** `apps/desktop/src/app/DesktopStartupRecovery.ts` opens a separate window when the backend cannot start (`show`, lines ~230-275). It uses `javascript: false`, `sandbox: true`, `contextIsolation: true`, and `setWindowOpenHandler` denies all windows. Buttons are plain links to a fake origin (`https://startup-recovery.invalid`); `will-navigate` cancels the navigation and runs the action. The offer comes from a pure function `resolveStartupRecoveryOffer` (lines ~41-60): prefer a newer build, then the last build that started on this machine, then the releases page. The last-started version is stored in a file (`desktop-last-started-version`). The window is skipped if the main window already exists, so a crash after launch is reported by the app itself.

**Why it helps Faber.** When the engine or a CLI fails at start, a tiny static page with three actions (update, reinstall previous, quit) is cheaper and more reliable than a full app screen that may itself depend on the failing component. It also keeps the failure path off the main screen (a product rule).

**Effort:** S/M.

**Kind:** Idea, plus the pure function `resolveStartupRecoveryOffer`, which is portable as logic. Credit T3 (MIT) if ported.

**Faber status:** No equivalent was found in `src-tauri`.

---

## 4. Packaged-app launch smoke test with a seeded profile

**What T3 does.** `scripts/smoke-desktop-app.ts` launches the **packaged** app with `_electron.launch`, against a temporary home seeded with an older-version SQLite snapshot (`fixtures/desktop-launch/statev2.sqlite`). It sets `T3CODE_TELEMETRY_ENABLED=false` and `T3CODE_DISABLE_AUTO_UPDATE=true`. It waits for a seeded thread in the sidebar and for its message to render, with a 60 s timeout per step. On failure it takes a screenshot, copies the logs, prints the last 20 lines of the backend log, and SIGKILLs a hung app after 10 s. The temp home is removed at the end.

**Why it helps Faber.** Faber's screen tests run in a hidden dev build. A release artifact can still fail to start (bad bundle, missing native dependency, migration). A launch test on the real `.exe` with an old-version profile covers upgrades too.

**Effort:** S (a launcher and one seeded fixture; the UI check can be a process-alive plus one window title check).

**Kind:** Idea. The pattern is portable; the Playwright code is not needed for Tauri.

**Faber status:** Not found in CI scripts. Check `.github` before building it, since the owner may already have something similar.

---

## 5. Update channel chosen by version pattern; mismatched updates ignored

**What T3 does.** `apps/desktop/src/updates/updateChannels.ts` (13 lines) is pure: a nightly pattern and a prerelease pattern on the version string, and `resolveDefaultDesktopUpdateChannel` returns `nightly` or `latest`. `DesktopUpdates.ts` lines ~453-465 set `allowPrerelease` and `allowDowngrade` only for nightly. Lines ~854-870 ignore an update whose version does not match the selected channel. Lines ~1040-1050: a mock feed on localhost, for testing the update path without a real release.

**Why it helps Faber.** Faber uses one endpoint (`tauri.conf.json`, `https://github.com/.../releases/latest/download/latest.json`, `installMode: quiet`). A channel guard (a build of the wrong train is never installed) and a local mock feed make update tests repeatable and keep prereleases away from stable users. Downgrade is allowed only on the channel that needs it.

**Effort:** S.

**Kind:** Pure function (`updateChannels.ts`). Port the logic, not the text; keep the MIT notice if copied.

**Faber status:** One endpoint, no channel guard was found in `tauri.conf.json`.

---

## 6. Install record and restart marker before the installer runs

**What T3 does.** `DesktopUpdates.ts` install path (roughly lines ~585-620 and ~740-760): before `quitAndInstall`, it writes a restart marker (`writeUpdateRestartMarker`) and an install record (`writeInstallRecord(targetVersion)`), then stops pooled backends with a 5 s timeout, then calls `quitAndInstall({isSilent, isForceRunAfter})`. On failure it restarts the backends. On the next start, `readNotAppliedInstallVersion` (~line 1033) reads the record and detects an install that never completed. `docs/internals/server-updates.md` describes the server side the same way: a durable pending record before ack, then "prepared", then a durable commit, and a rollback that restores a database snapshot.

**Why it helps Faber.** The window between "installer started" and "new version running" is where updates get lost. A small durable record that is written first and checked on start tells the app whether the update was applied, without guessing from version numbers.

**Effort:** M.

**Kind:** Idea. Faber already has this shape for local builds (`src-tauri/src/local_update.rs`: probation at lines ~576-640 and ~2019-2094, a rollback checkpoint at `rollback.json`). What is missing is the same record around the **installer** step for the published release.

---

## 7. Renderer memory sampled at a 30 s floor, guarded by PID identity

**What T3 does.** `apps/desktop/src/telemetry/DesktopRendererHistory.ts`, `recordMetrics` (lines ~240-262): records at most every 30 s (`sampledAtUnixMs - lastSampledAtUnixMs < 30_000`). It matches a renderer to its process metric by PID only when the PID source is "current", and it stores `creationTime` so a reused PID is detectable. Memory is working set and peak working set, in bytes, per surface (main, preview, splash, picture-in-picture, devtools). `DesktopTelemetryPublisher.ts` line ~215 uses one Electron sampler for the shared CPU interval.

**Why it helps Faber.** Faber's budget is tested per process. WebView2 renderer processes come and go, so the same identity guard (PID plus creation time) is needed before attributing memory to a surface.

**Effort:** S.

**Kind:** Idea.

**Faber status:** Not found. Tauri on Windows uses WebView2, so the process list is the source, not an Electron API.

---

## 8. Foreground activation with thread-input attach, as a pure function

**What T3 does.** `apps/desktop/src/electron/WindowsForeground.ts`, `activateWindowsForegroundWithApi` (lines ~25-45): returns true if the target is already foreground; otherwise it attaches the current thread's input to the foreground window's thread, calls `SetForegroundWindow`, and detaches in `finally`. The Windows API is injected through the `WindowsForegroundApi` interface, so the function is testable without Windows. It also treats `ApplicationFrameWindow` (UWP-hosted) specially. A separate worker (`WindowsForegroundFocusWorker.ts`) uses an accessibility library to find the element for a window and focus it when `SetForegroundWindow` alone is not enough.

**Why it helps Faber.** A notification or deep link that should bring Faber to the front is a common Windows failure: the OS blocks focus stealing. The attach-then-set pattern is the known workaround, and injecting the API makes it unit-testable (Faber's tests must never touch real windows).

**Effort:** S.

**Kind:** Pure function (`activateWindowsForegroundWithApi`) with an injected API. Re-implement in Rust with a trait so the logic is testable. Do not add the accessibility worker: it adds a dependency and CPU cost that Faber's budget does not justify without a measured need.

---

## 9. Hidden-first window, revealed after first paint

**What T3 does.** `DesktopWindow.ts` lines ~378-390: the main window is created with `show: false` and `backgroundThrottling: false` for the first paint, with a comment that hidden-renderer throttling stalls first paint and that throttling is re-enabled after the first reveal.

**Why it helps Faber.** Faber starts with a tray icon. A window that starts hidden and is revealed only when asked avoids a blank flash, and the throttling note tells you when to turn background throttling back on (to save CPU when hidden).

**Effort:** S.

**Kind:** Idea. Check Faber's Tauri `visible: false` config and the throttle behaviour before changing anything; measure first paint.

---

## 10. Single-instance lock that queues second-instance arguments

**What T3 does.** `DesktopClerk.ts` lines ~109-119: `requestSingleInstanceLock`; if the lock is lost, the second instance quits. The second-instance handler queues argument lists (`pendingSecondInstances`) until the runtime is ready (lines ~222-237), then replays them.

**Why it helps Faber.** A second launch (for example from a deep link or a shortcut) should hand its arguments to the running app, not be dropped or start a second engine.

**Effort:** S.

**Kind:** Idea.

**Faber status:** Not checked in detail; confirm before proposing.

---

## 11. Installer verifies SHA256 before extraction; staging, then atomic move

**What T3 does.** `scripts/install.ps1` (lines ~150-234): downloads `SHA256SUMS` and the archive into a random staging directory; matches the archive name with a regex; compares `Get-FileHash` to the expected hash **before** extracting; extracts into staging; runs the new `t3.exe --version` as a smoke test; writes `.install-complete` with the version; then moves the directory into place (removing an existing one first). On failure the staging directory is deleted. The `t3.cmd` shim is written as UTF-8 without BOM, with a comment that cmd.exe reads it byte for byte. A launcher registry file (`runtime/launchers`) keeps the shim path, so pruning old versions cannot break the command.

**Why it helps Faber.** Faber's Windows installer should not extract a file it has not verified, and an interrupted install should leave nothing half-written. The marker-plus-atomic-move pattern is the same idea as item 6.

**Effort:** S.

**Kind:** Idea. Faber ships through Tauri's updater, so this matters only if Faber ships a separate CLI or helper archive.

---

## 12. Native load probe for the packaged Windows binary on the target arch

**What T3 does.** `scripts/verify-windows-desktop-artifact.ts` (48 lines) runs the native-load probes that a cross-built artifact skipped, on a host of the target architecture (`arm64` or `x64`), through `verifyWindowsPackagedRuntime` in `build-desktop-artifact.ts`. CI bundles it with the build script.

**Why it helps Faber.** Cross-building for Windows can produce a binary that compiles and then fails to load a DLL on the real machine. A load probe on the target arch catches that before release.

**Effort:** S.

**Kind:** Idea.

---

## What not to copy

- **Windows tree kill.** `packages/provider-acp/src/server/AcpSessionRuntime.ts` (~1021-1060) and `packages/provider-pi/src/server/rpc.ts` (~185-230) run `taskkill /PID <pid> /T /F` without `windowsHide`, and the Pi version ignores failure. T3 also has a plain `SIGTERM` plus `forceKillAfter` path for the desktop backend (`DesktopBackendManager.ts` ~441-442) with no tree kill on Windows, so a grandchild process can outlive the server. **Faber is already ahead here:** `winproc` starts CLIs suspended, puts them in a job object with kill-on-close, and resumes them (`winproc/src/lib.rs`); `engine/src/proc.rs` falls back to `taskkill` with `CREATE_NO_WINDOW` (`0x0800_0000`) only when a process is in no job. Keep that design. Do not copy T3's unhidden `taskkill`, which would open a console window on the owner's PC.
- **Electron-specific window and IPC code.** Does not apply to Tauri 2.
- **Background service on Windows.** T3 does not support it; Faber should not treat T3 as a model there.

## Measurement notes

- T3 measures resources with a separate sidecar (`native/resource-monitor`, Rust, sysinfo 0.39.3, JSON-lines protocol v3, sampling only when asked). Its own doc (`docs/internals/resource-telemetry.md`) lists the traps: PID reuse (use start time as identity), snapshot sequence per monitor generation, and Windows I/O semantics. WSL needs a Linux monitor, which is not shipped in Windows packages.
- Renderer memory (item 7) is taken from Electron's app metrics every 30 s at most; the telemetry publisher's cadence (item 1) changes with lock, suspend and battery.
- For Faber, the cheapest first measurement is: idle wakeups per minute and working set of the whole process tree, with the screen locked and unlocked, before and after any sampler change.

## Attribution

T3 Code is MIT-licensed (per the brief). Items 1, 3, 5 and 8 name portable pure functions. Re-implement the logic in Rust from the description above rather than copying the TypeScript. If any text or table is copied, keep the MIT copyright and license notice in the copied file. Faber's own rule is original code only, so the safe default is to reuse the idea and write it fresh.

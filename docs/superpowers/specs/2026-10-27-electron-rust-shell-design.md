# Electron shell with everything else in Rust

Status: Decided by the owner (R459); plan proposed by AI. Started 2026-10-29 (the shared command table, below). It
replaces the Tauri shell (`src-tauri/`) and supersedes R31 ("Node never in the installer"). R125's memory limits stay.

Owner, 2026-10-29, on the phase 1 stop gate: "Pode passar o ideal é só ser otimizado n precisa seguir um numero fixo só
deixe tudo o mais otimizado que der sem memory leak cpu leak e nem nada assim". So no idle number stops the move; every
phase measures, cuts what it can, and treats any memory or processor growth over time as a bug.

## Request and intended result

Owner request, preserved verbatim:

> Coisas assim tipo playwright que dá pra reescrever/clean-room ou fazer o proprio sei lá para deixar mais leve tipo
> rust eu quero que sejam em rust ou coisas do tipo para ser o mais otimizado possivel e vou migrar pra electron msm

> Escreve o plano da migração para Electron com tudo que der em Rust, e registra a decisão.

Result: Faber runs in Electron, so one Chromium draws the screen the same way on Windows and macOS, the screen
scenarios and the agent's browser pane work on both, and design never waits for WebKit. Everything that is not
Chromium, V8 or the screen itself is Rust: the engine, the shell's logic, the test runner, markdown and code colours,
the updater and the project scripts.

## Why, and what it costs

Reasons, from the 2026-10-27 discussion:

- One engine everywhere. Tauri draws with WebView2 on Windows and WebKit on macOS. WebKit draws some effects and fonts
  differently and gets new CSS later, so every screen is tested on two engines.
- macOS testing. WKWebView has no DevTools port, so `testkit` skips every screen scenario on macOS. Electron has one
  on every system.
- The browser pane on macOS. `src-tauri/src/pane/native.rs` drives the pane through a call only WebView2 has.
  Electron's `webContents.debugger` sends the same DevTools Protocol from inside the app, with no port, on every system.

Costs, stated before the decision:

- A fixed floor of roughly 100-200 MB at idle for Chromium plus Node, and an installer of about 100 MB instead of
  about 10 MB. These are estimates; phase 1 measures them.
- Node ships again (R31 reversed). Node only relays calls; no product logic runs in it.
- The leak measured on 2026-10-27 is not Tauri's. The installed Faber (`E:\Apps\Faber`, open 3 h) used about 1.3 GB
  of private memory: 707 MB in `faber.exe`, which is the Rust engine and shell, plus about 600 MB in WebView2.
  `docs/product/architecture.md` says 89 MB idle. The engine moves into Electron unchanged, so the leak moves with it
  unless phase 0 fixes it.

## Architecture

```text
Electron main process (TypeScript, thin: windows, menus, IPC relay)
├── faber-native.node  (Rust, napi-rs: the engine + the shell's logic, one tokio runtime)
├── preload.ts         (contextBridge: the same fixed command list the screen has today)
├── BrowserWindow      (SolidJS screen from dist/, unchanged; sandbox on, no Node in the page)
└── WebContentsView    (the agent's browser pane, driven by webContents.debugger)
```

- **The engine runs in the main process as a napi-rs addon.** This saves the process and serialization of a sidecar,
  and the engine already returns keys (`error.*`) and JSON-shaped models. napi-rs turns a Rust panic at the boundary
  into a JS error, so the addon builds with `panic = "unwind"`; the rest of the release profile stays. Phase 1 measures
  the addon against a sidecar process (the same engine behind stdio), and the sidecar wins if the addon costs more
  memory or makes a crash take the window down.
- **Node is a relay only.** `electron/main.ts` creates the window, the tray and the pane, and forwards each command to
  the addon. A rule that decides anything lives in Rust and has a Rust test. The size target is under 800 lines of
  TypeScript in `electron/`.
- **One command table for every front end.** The commands live in the `host/` crate (`faber-host`): one `table!`
  list builds each command, `Host::dispatch(name, args JSON)` and `Host::commands()` (name, camelCase arguments, and a
  kind: read, write, starts a turn, or needs a window), so they cannot drift. The Tauri shell relays every table command
  to it before Electron exists, which proves the table on the screen suite first; then `preload.ts` exposes the same
  `invoke(name, args)`, and the command line (`faber`, its own spec) calls it over a user-only pipe. Out of process the
  table speaks newline-delimited JSON (`host/src/wire.rs`): `{"id","cmd","args"}`, `{"id","ok"}` or `{"id","err"}`,
  and `{"event"}`. What only a window can do (the pane, dialogs, the tray) goes through the `Shell` trait each front end
  implements. The screen reaches the shell in three places only (`invoke`, `listen`, the window buttons), so
  `ui/api.ts` swaps one import.
- **Security.** `contextIsolation`, `sandbox` and no `nodeIntegration` in the screen and the pane. The Electron fuses
  that turn off `ELECTRON_RUN_AS_NODE`, the Node options and the inspector arguments in a release build. The screen
  loads only `dist/` through a custom protocol, never `file://` or a remote page. A link opens outside the app. These
  are two-review security changes (R146).

## Where each part of `src-tauri/` goes

| Today (`src-tauri/src/`) | Lines | In Electron |
|---|---|---|
| `main.rs` (window, commands) | 1799 | Commands become napi-rs exports in `native/`; window code goes to `electron/main.ts` |
| `look.rs` (appearance to CSS variables) | 1126 | Unchanged Rust in the addon |
| `tray/decide.rs`, `tray/badge.rs` | 1142 | Unchanged Rust; the badge is drawn in Rust as PNG bytes for `nativeImage` |
| `tray.rs`, `tray/native.rs`, `tray/sound.rs` | 544 | Electron `Tray` and `Notification`; the windowless state queries stay in Rust |
| `awake.rs` (keep the PC awake) | 656 | Electron `powerSaveBlocker`; the Windows API file goes |
| `autostart.rs` | 348 | `app.setLoginItemSettings`; the Windows API file goes |
| `pane/mod.rs`, `pane/native.rs` | 672 | `WebContentsView` + `webContents.debugger`; the rules of `mod.rs` stay Rust |
| `update.rs`, `local_update.rs`, `bin/faber-local.rs` | 3614 | Stay Rust; the signed download replaces `tauri-plugin-updater` (see below) |
| `setup.rs`, `folder.rs`, `terminal.rs`, `project_commands.rs` | 618 | Unchanged Rust in the addon |

Two of the four files that may use unsafe code (`awake.rs`, `autostart.rs`) go, because Electron already wraps those
calls. `winproc/` stays.

## Everything else in Rust

| Piece | Today | After |
|---|---|---|
| Screen test runner | `testkit` drives WebView2 by DevTools, Windows only | The same runner drives Electron by DevTools on Windows, macOS and Linux (`--remote-debugging-port=0` and the `DevToolsActivePort` file, or a pipe if Electron accepts `--remote-debugging-pipe`) |
| What Playwright has that the runner lacks | none | A trace on failure (page, log and a screenshot per step in one file), screenshot comparison with a tolerance, and locators by role and text. Clean-room, written from the protocol, never from Playwright's code |
| Markdown | `marked` + DOMPurify in the screen | `comrak` in the engine, with code colours from `syntect` or `tree-sitter-highlight`; the screen receives finished, safe blocks |
| Updater | Rust + `tauri-plugin-updater` | Rust: signed manifest, download, signature check (`minisign`), the existing probation and rollback |
| Project scripts | `scripts/land.mjs`, `tasks.mjs`, `base.mjs`, `ci-scope.mjs`, `screen-costs.mjs` | `testkit` subcommands; `ci-scope` reads `testkit/suites.json` through `plan.rs` instead of a copy |
| Build tools | Vite 8 (Rolldown), oxlint, TypeScript 7 | Already native; no change |
| Packaging | Tauri bundler (NSIS) | `electron-builder` at build time only (development tool, never shipped); a Rust packager only if it proves slower or larger |

Not rewritten: Chromium, V8, Electron itself, and the scripts that parse the TypeScript locales (`numbers.mjs`,
`release-notes.mjs`, `woff2-only.mjs`).

## Keeping Electron light

- One window, one renderer. The pane is a `WebContentsView` created only when it is opened and destroyed when it is
  closed, like today.
- Closed to the tray, the window is destroyed and the renderer freed (today 4 MB in the tray). Only the main process and
  the addon stay.
- Chromium features Faber does not use stay off: spellcheck, the PDF viewer, WebSQL and others the measurement shows.
  `backgroundThrottling` stays on.
- V8 code cache for the screen bundle, and the screen keeps drawing only the visible messages.
- Every heavy rule (markdown, colours, the worklog tidying in `ui/worklog.ts`, the grouping in `ui/sidebar.ts`) runs in
  Rust, so the renderer only draws.

## Phases

Each phase is its own pull request through `land.mjs`, with its measured memory and processor cost in the body.

0. **Find and fix the leak, and add a long scenario.** A screen scenario that uses the app for a long compressed session
   (many turns, conversations opened and closed, the pane opened and closed) and fails when private memory grows
   past a set slope. It runs on the Tauri build first, so the leak is fixed before the move and both shells are measured
   from the same baseline.
1. **Skeleton and measurement.** `electron/` + `native/` (napi-rs) open the real screen with the engine, with no other
   feature. Measure private memory idle, with 1000 messages and in the tray, cold start and installer size on Windows
   and macOS, addon against sidecar. The numbers go in this spec and the pull request; no fixed number stops the move
   (owner, 2026-10-29), but every cost found is cut before phase 2 ends.
2. **Commands.** Every command moves into `host/` (started on Tauri, see above), the Electron front end relays them,
   and the checks in `tests/ui` pass in Electron.
3. **Runner.** `testkit` launches Electron on the hidden desktop with the same sandbox and audits, the screen suite runs
   on Windows, macOS and Linux, and trace, screenshot comparison and locators are added.
4. **Tray, notifications, keep-awake, autostart, single instance, dialogs.**
5. **The browser pane** on `WebContentsView` + `webContents.debugger` on both systems. Its no-port rule (R195) stays.
6. **Updater and installer**, signed. An installed Tauri Faber updates itself to the Electron one once, with the existing
   probation and rollback. Settings and conversations stay where they are, because the engine owns them.
7. **Remove `src-tauri/`**, then update `docs/product/architecture.md`, `docs/dev/testing.md`, AGENTS.md (stack, the
   unsafe file list) and the product map.
8. **Rust in place of the screen's logic and the scripts**, piece by piece: markdown and colours, the worklog,
   the sidebar grouping, then the scripts. Each piece can land on its own after phase 2.

## Risks

- **Memory.** The fixed Electron floor may not fit R125 together with the conversation. Phase 1 stops the move if it
  does not.
- **Panics in the addon.** A panic napi-rs does not catch, or one inside a tokio task, ends the main process and the
  window. The sidecar fallback of phase 1 covers it.
- **Security surface.** Node in the main process is a new place for an attacker to aim. The fuses, the sandbox and the
  fixed bridge are reviewed twice.
- **Signing and notarization on macOS** need the owner's Apple account. They are not a new cost, but they are asked for
  when phase 6 starts.

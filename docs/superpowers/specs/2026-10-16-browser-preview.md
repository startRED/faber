# Browser preview: the screen without the shell

Status: proposal, nothing built. Recommendation: yes, as a small developer tool, behind the visual critique (R168).

## The question

Can an agent edit `ui/`, see the result and photograph it in seconds in a browser pane, with Vite and the testkit's fake
CLI and no Tauri window? Today a look at a changed screen means a test build of the shell (`cargo t --suite screen`):
a cold build of minutes, then about ten seconds per launch. Vite alone (`npm run ui:dev`) starts in under a second, but
the page stops at its first `invoke('state')`, since `@tauri-apps/api` needs the shell's `__TAURI_INTERNALS__`.

## What it would take

The screen talks to the shell only through `ui/api.ts` (`invoke` and `listen` of `@tauri-apps/api`), 65 commands
(`generate_handler!` in `src-tauri/src/main.rs`) and two events. Two ways to answer them:

1. **A bridge to the real engine.** A small test-only binary in `testkit/` (`preview`) runs `faber_engine` in a sandbox
   like a screen test (temporary home, empty `PATH`, the fake CLI replaying `tests/fixtures/`), serves the commands over
   one loopback WebSocket, and Vite, in a new `preview` mode, installs a `__TAURI_INTERNALS__` shim that forwards
   `invoke` and `listen` there. The engine is real, so conversations, quota cards, permission cards and missions behave
   as they do in the app. The cost is a second dispatch table beside `generate_handler!`, which drifts unless it is
   generated from it or a test compares the two lists.
2. **Canned state.** The shim answers from JSON files (`tests/fixtures/preview/*.json`) and never starts the engine.
   Cheapest and fastest, but every flow that streams or changes state (sending, a permission card, a mission) needs
   hand-written replays, and a canned answer drifts from the engine's shape without a test.

Option 1 is the useful one: the screens a critique looks at (homes, a conversation, Settings) all need `state`, and
the streamed answer is where most visual defects live.

## Cost

Estimates, not measured; the lane that builds it measures them.

- Memory: Vite's dev server about 150 MB, the engine with the fake CLI about 40 MB, a browser tab of the screen about
  150 MB; the agent's browser pane is already open. Against a screen test, which holds the test shell and its WebView2
  (about 300 MB) plus the runner, it is lighter, and it never needs a test build of the shell.
- Processor: no Rust build of the shell; the bridge builds once with the engine (shared by sccache). A change to a
  `.tsx` file repaints in about 100 ms through Vite's hot reload, against a full relaunch now.
- Disk: none beyond the engine build every lane already has.

## Risks

- **Two shells.** WebView2 and the agent's browser are both Chromium, but fonts, scrollbars, the window frame, the title
  bar and DPI differ. A preview photo is a draft: the approved photo of a screen card and every gate stay with the
  screen tests in the real shell.
- **Drift.** Option 1 must list the same commands as the shell; a test that reads both lists fails when one gains a
  command. Option 2 drifts without that test.
- **Containment.** The bridge listens on loopback only, with a random token in the page's address, and starts the fake
  CLI only, never a real one; no CLI credential is read (AGENTS.md). It must run hidden like the rest (C006): it opens no
  window, and the browser pane is the agent's own.
- **Scope creep.** It is not a web version of Faber. It ships in no build and stays a test-only binary.

## Recommendation

Yes, option 1, as one small lane once the visual critique has run for a week and its slow step is measured: if most of
the critique's time is the screen-test build, the preview pays for itself. It needs the bridge binary, the shim in a
Vite `preview` mode, the test that keeps the two command lists equal, and a line in design.md saying a preview photo
never replaces the screen test's.

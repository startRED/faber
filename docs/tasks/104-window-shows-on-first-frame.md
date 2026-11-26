# The window shows when its first frame is drawn, and a test holds the start time

Area: Window opens
Files: ui/main.tsx, electron/main.ts, electron/startup.ts, scripts/electron-startup.test.mjs, tests/, docs/product-map.md
Depends: start-answers-before-history-loads
Size: about 300 changed lines

Why: plan item A1, second and third parts. In the owner's `startup.log` the window shows at about 3.3 s
`by=failsafe` in 41 of 45 starts, and in 4 it never showed. Read on 2026-11-25: `ui/main.tsx` calls
`api.screenReady()` only after `await api.state()`, which waits for the host, so with a slow engine the paint signal
always loses to `REVEAL_FAILSAFE_MS` in `electron/main.ts`. `screen_ready` is a window command the shell answers by
itself (`windowCommands`), so it does not need the host. `ui/index.html` already has a static first frame (`#first`).

Build:

- The screen tells the shell it painted once the static first frame is on screen, before it waits for the host; the
  conversation id that `screen_ready` returns is still delivered once the real screen mounts.
- Find why 4 starts never showed (a hidden `--autostart` start is expected; anything else is a bug) and fix it.
- A ceiling a check enforces: on the built test app, `startup.log`'s `shown` comes `by=paint`, and the host answers
  within a bound written in the check with the worst value seen on this PC in its comment (the docs ask for 3 s median,
  10 s worst on a real profile; the check uses an empty test profile and a tighter bound).

Leave out: the engine's start (task start-answers-before-history-loads).

Done: the check fails on today's code (shown by failsafe) and passes after; `scripts/electron-startup.test.mjs` covers
the new order. Extends the product-map row `Window opens`. Screen and shell only: ships on its proofs.

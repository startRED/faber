# Proof of concept: read and act on another program's window in the background, in five programs

Area: none
Files: winproc/examples/eyes.rs, winproc/Cargo.toml, docs/dev/eyes-proof.md
Depends: none
Size: about 450 changed lines

Why: plan item E0, the most uncertain piece of the next wave, and the owner's answers 1 to 4 of 2026-11-25 (eyes in
this update, hands after the signed installer, following the conversation's permission mode, with brakes in code). Its
result decides the design of E1 to E3. Read on 2026-11-25: `winproc/` holds Faber's unsafe Windows calls and already has
`full_screen_in_front`; tests use an invisible desktop (`testkit/src/desktop.rs`); rule C006 forbids a visible window
or taking the focus (`docs/product-rules.md`). Known mechanisms (from outside, mechanism only): window capture
(Windows.Graphics.Capture, PrintWindow as fallback), UI Automation for the tree, input through UI Automation patterns
and window messages, SendInput only as a last resort.

Build:

- An example program (not part of the app, not run by any test) that, given a window, prints its UI Automation tree as
  short text, saves a picture of it without bringing it forward, and performs one action in the background (invoke a
  button, set a text) without moving the mouse or the focus.
- Run it on five kinds of program, on the invisible desktop or on windows the example opens itself: a classic Win32
  app, a modern (WinUI/UWP) app, an Electron app, a browser, and a custom-drawn or game-like window. Close what it
  opened.
- `docs/dev/eyes-proof.md`: per program, whether tree, picture and background action worked, time and size of each, and
  what failed; a recommendation for E1 and E2 in five lines.

Leave out: any change to the app, a tool for the AI, anything that takes the mouse or the focus, any program the
person has open.

Done: the page with the five measured results; `cargo build -p faber-winproc --examples` passes; nothing new runs in
`cargo t`. An experiment: ships on its proofs (unsafe code stays in `winproc/`).

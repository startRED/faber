# UX sentinel

The screen sentinel and the visual critique.

## UX sentinel

The sentinel (`testkit/src/sentinel.rs`, `testkit/src/probe.js`, `testkit/src/screen/sentinel.rs`) judges what a person
would see, not what the page says. It is a set of screen scenarios, so it runs hidden, sandboxed and audited like the
rest. The design is in `docs/superpowers/specs/2026-10-12-ux-sentinel-design.md`; this is slice S0.

- **Probe and rules.** The probe is injected over the DevTools port. It only measures: box, opacity and presence of every
  `data-surface` element on each frame (exit copies in shadow roots included), and every animation that starts. The
  rules are pure functions on those numbers, unit tested with synthetic samples. Each rule has a planted twin: a
  defect that must be found, injected into the page of the sandbox (never into the repository). A scenario fails if
  its twin is not found.
- **Clock and slow motion.** Before a motion scenario, a 1 s transition on a hidden element must give at least 45 frames
  with a median under 20 ms and a `transitionend` (three tries). Then `Animation.setPlaybackRate(0.25)` makes every
  animation 4 times slower, so a panel that jumps is seen as one frame. The test build starts with three browser switches
  that stop the engine from slowing frames on a window no monitor shows.
- **Scenarios.** Panels move through frames and reduced motion moves nothing (M1, M6). A permission card arrives
  through frames (M1, dialogs). The composer grows and shrinks and the end of the thread stays clear (I1 to I5, L4,
  L5; the cap is measured against the window height, 40vh). No text is garbled in five languages, and a title with `› « » ß ç ã` survives the engine's file (T1; the static scan in
  `tests/ui/mojibake.check.ts` covers every locale and source string). The main screens keep their composition at
  1920x1080 and 1280x800 (V1 to V3, below).
- **Composition.** V1: no text a person reads sits on lit art. The screen is photographed twice with its letters made
  transparent, with and without the art (`.ambient`, `canvas.amb-hole`), and the light the art adds under each text's
  glyph boxes (`Range.getClientRects()`, 99th percentile of the brightest channel, so a grain of dust does not count)
  stays at or under 77 of 255, the bound of `tests/ui/hole.check.ts`. V2: a name the person gave (a project, a
  conversation) or a heading shows in at most two places; a kind of element is one place, so a list naming the project
  on each row is one. V3: two side-by-side columns of a grid, each at least 240x120 px, end within 8 px of each other;
  a frame that runs to the window's bottom, or a column that scrolls, does not count. All three are high.
- **Findings and the gate.** A finding is the hash of kind, screen, surface and element. Only critical and high
  findings whose hash is not in `tests/ux/baseline.json` fail a run; medium ones are written down and become tasks. The
  file is a ratchet: it lists what is known, and a fix removes its line. Findings go to
  `target/faber-test/ux/<scenario>.json` and raw frame traces to `target/faber-test/ux/traces/`. Nothing records the
  person's screen.

## Visual critique

Checks see code and motion, not a screen as a whole (R172). The sentinel's scenario "the main screens keep their
composition" photographs the Chat home, a project's home, an open conversation and Settings at 1920x1080 and 1280x800 in
Dark Room and pt-BR (`target/faber-test/shots/critique-<screen>-<size>.png`). `node scripts/shots.mjs <label>` runs it and
copies the photos of the screens the diff touches into `target/faber-test/critique/<label>/` with an index;
`scripts/shots.test.mjs` keeps the choice of screens. A fresh session scores the `before` and `after` folders out of 40
with `.claude/skills/visual-critique/SKILL.md`; the score never drops and a touched screen reaches 32.

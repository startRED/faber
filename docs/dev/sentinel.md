# UX sentinel

The screen sentinel and the visual critique.

## UX sentinel

The sentinel (`testkit/src/sentinel.rs`, `testkit/src/probe.js`, `testkit/src/screen/sentinel.rs`) judges what a person
would see, not what the page says. It is a set of screen scenarios, so it runs hidden, sandboxed and audited like the
rest. The design is in `docs/history/superpowers/specs/2026-10-12-ux-sentinel-design.md`; this is slice S0.

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
  `target/faber-test/ux/<scenario>.json` and all raw frame traces to `target/faber-test/shots/rehearsal/traces/`,
  including incomplete interactions without photographs. Every motion sentinel retries an infrastructure result (lost frames, a frame clock the runner did not give) once in a fresh sandbox; two incomplete attempts are reported as a named quarantined failure, while completed motion failures still block. CI uploads traces in its screenshot artifact. Nothing records the
  person's screen.

## Interaction rehearsal

The existing panel-motion scenario also photographs the real hidden app before, during and after opening and closing
the running column at 720 and 1440 px, opening and closing Settings, applying Morning and Dark Room through the actual
appearance controls, and receiving the recorded sky answer. That replay preserves the fixture's words and events,
with 120 ms pauses between its text deltas so partial words can actually be photographed. No real provider runs.

Each interaction writes its own `target/faber-test/shots/rehearsal/<interaction>-<run>/index.html` contact sheet, JPEGs and
`rehearsal.json`, including the frame trace, photograph intervals, coverage status, supported long-task measurements
and reason for incomplete evidence.
A second connection to the same hidden app takes photographs while the first drives the interaction; it closes when
the interaction ends. Images keep the viewport's full resolution, quality 60, at most 96 per interaction, with an 80 ms
pause between captures. Scaled clips are forbidden: WebView2 changes the live viewport while capturing them. Each
photograph records and checks the original viewport before and after capture. This is test-only work: no recorder,
extra state or process runs in the shipped app.
The probe follows exit animations inside their shadow trees too; streaming photographs reject any visible Settings
surface, including its exit copy. Photograph metadata retains those copies and their animation states for diagnosis.
The existing CI screenshot artifact includes these bundles recursively; each JSON embeds its complete raw trace.

Calibration alone cannot qualify a run. Every watched interaction needs a surface actually seen and valid ordered
samples from its start through its settled end, with no frame gap over 100 ms. A trace that times out while geometry or
a finite animation still moves is incomplete and fails the scenario, including under reduced motion. Photographs need before/during/after coverage,
no interval or gap over 500 ms, and successful file writes; streaming also needs an observed partial reply and a longer
completed reply. Missing or incomplete evidence never passes. The contact sheet says `ready-for-review`, not visually
passed: an independent reviewer still inspects alignment, clipping and continuity. The original geometry rules and
planted-defect gates continue to run. Run only this scenario with
`node scripts/cargo.mjs t --filter "sentinel: panels move through frames"`; all scenarios run in CI as before.

## Visual critique

Checks see code and motion, not a screen as a whole (R172). The sentinel's scenario "the main screens keep their
composition" photographs the Chat home, a project's home, an open conversation and Settings at 1920x1080 and 1280x800 in
Dark Room and pt-BR (`target/faber-test/shots/critique-<screen>-<size>.png`). `node scripts/shots.mjs <label>` runs it and
copies the photos of the screens the diff touches into `target/faber-test/critique/<label>/` with an index;
`scripts/shots.test.mjs` keeps the choice of screens. A fresh session scores the `before` and `after` folders out of 40
with `.claude/skills/visual-critique/SKILL.md`; the score never drops and a touched screen reaches 32.
`node scripts/council.mjs <label>` is that scorer: judges of several vendors score the same photos at once, a line is
their median, a judge far from the middle one is a jester whose numbers leave the score, and only rows two judges agree
on are work. `--refs` adds photos of other apps as the bar, and `--against <label>` says whether a climb keeps a round,
reverts it or stops at its target (36 of 40). `scripts/council.test.mjs` keeps the scoring; the skill says how to climb.

## First-user review

Checks and the critique know what Faber should look like; a newcomer only knows what the screen says (R244).
`node scripts/first-user.mjs` starts the hidden test build through `testkit/examples/ux_audit.rs` (first run, no CLI,
720x480, pt-BR by default; `--size`, `--lang`, `--steps`, `--no-build`) and gives a fresh AI one step at a time: a
screenshot and the controls a screen reader would name, numbered. Text is read from the image, never copied from the
page body: that would also reveal text below the viewport. It answers with one action (click a
number, select an option, type, a key, scroll, wait, done or stuck) and what confused it. A reachable native selector
exposes its accessible option labels; selection requires an exact, unique enabled label in the observation and rechecks
the same connected, reachable element and enabled option before dispatching input and change events. Wait uses the same fixed 1.2-second pause as
other actions and counts toward the step limit; it cannot request an unlimited delay. The default disconnected Chat
task discovers what is needed to start; it never requests account connection or an unavailable response. The tasks are the first run, Chat, Projects
and the language in Settings (`TASKS`), in one app, each starting where the last ended. The Catalog task comes after
onboarding: find a debugging tool, read its purpose and automatic-use explanation, then return to the list. This task
only browses; it does not test installation, removal or switches. `--task catalogo` runs this task alone, while an
unknown task name fails before launching the app. Other task ids are in `TASKS`.

`--task conversa` is a separate prepared Chat walk with the existing fake replay, without first run or a seeded project.
The reviewer chooses Claude Haiku, sends the recording's exact sky question and follows the recorded response. Its
report labels this prepared state; it is not first-opening, account-connection or project-conversation evidence.

`--task conversa-projeto` is an opt-in controlled project walk, outside the default tasks. The driver seeds the existing
isolated Demo repository and enables the fake chat replay (`--project-demo`, incompatible with `--first-run` or
`--no-ai`). The reviewer opens Demo, chooses Claude Haiku and sends the recording's exact sky question. This proves
finding a prepared project conversation and following its recorded answer; it does not prove adding a project through
the native folder picker, Auto routing, real-provider compatibility, file edits or checks. No real CLI runs inside the
app; the reviewer still uses quota. The driver must be rebuilt when changing this setup; an old cached app is not new
source evidence.
Before asking the reviewer, the driver checks the document's actual language against `--lang`; a mismatch saves an
image and fails instead of writing a report under the wrong language.

The reviewer never sees the code. The default is Haiku through `claude -p --safe-mode --tools ""` with no MCP server, which starts
with no tool at all (its `init` line lists none); safe mode disables customizations but keeps normal authentication, model selection
and permissions. Managed settings policy still applies. `--agent codex` is gpt-6-luna with its shell, apps, browser and
computer use switched off and a read-only sandbox. Either runs in an empty folder under the system temp folder, and the
script refuses one inside the repository. Builds run at idle priority, the reviewer's processes have no window, and no
CLI runs inside Faber, so its only cost is the reviewer's own calls (about 40 small Haiku turns).

Each run gets a unique `<date>-<suffix>` name, including repeat runs with the same configuration. The findings go to
`docs/history/research/first-user-<date>-<suffix>.md`, most serious first, each with what it tried, where it got stuck and the
step's screenshot in `target/faber-test/first-user/<date>-<suffix>/` (local only). `--out` overrides only the report;
screenshots still get their own folder. Each actual control list is saved beside its screenshot before the reviewer receives
it, using exclusive creation so a previous step's evidence cannot be replaced. The run also records streamed SHA-256
identities for the driver, app and fake CLI before and after the driver build and around the app launch. A binary that is
absent before a build is recorded as null; other read errors stop the review. With `--no-build`, the app may be reused, so
compare the recorded launch-boundary identities before treating it as built from current sources. Nothing is fixed from it directly:
a person reads the list and turns what is real into tasks. A reviewer's `done` is labeled as its own claim, requiring
image inspection; it is not verified completion. `scripts/first-user.test.mjs` keeps the answer parsing, the
folder check and the report.

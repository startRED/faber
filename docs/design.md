# Faber design

Decided on 2026-10-11. This is the direction that Phase 1 builds. It replaces the Mono design of 2026-10-10. The
approved mock (six screens) is the visual reference, and each Phase 1 piece updates this file for its own change.

## One structure, many looks

There is one screen structure. Every theme is only a set of token overrides on a shared base. Every colour, size,
radius, duration and shadow comes from a token.

### Themes

There are three themes. Sala escura is the default.

| Theme (English name) | What it looks like |
| :--- | :--- |
| Sala escura (Dark Room) | Black and white with strong contrast and a projector light on the new conversation |
| Oficina (Workshop) | Warm graphite, book-serif headings, chalk-blue details and a slow loom effect |
| Bancada (Workbench) | Dark olive, narrow headings, mint details and a flow line from idea to proof |

Theme and preset names are translated like every other text. Portuguese (Brazil) keeps the names above.

### Presets

Three presets sit on top of the themes:

- **Manhã (Morning):** the light look.
- **Alto contraste (High contrast):** high contrast. It starts in reduced motion.
- **Cobalto (Cobalt):** navy and gold, inspired by the TL-ADE.

There is no light/dark switch that follows the system. The light look is the Morning preset.

### Customization

The user can change:

- the accent, from five swatches or any colour;
- the ground (black, warm graphite, olive, navy or light paper);
- the text font and the heading font;
- the text size, from 13 to 18 px;
- the density and the corners;
- the motion level: full, reduced or none;
- three effect switches: ambient, texture and glow.

Choosing a theme or a preset keeps the text size, the density and the motion level, and resets the rest, so the new
theme shows as designed.

A custom look saves to a small file that can be shared (`.faber-theme.json`). The file stores choices, never CSS. It is
checked when it is opened: an unknown key, a wrong type, a value outside the allowed list or a file over 20,000 bytes
is refused, and nothing changes. One exception: keys that start with `amber`, `claude` or `codex` inside `colors` are
dropped without failing, because the meaning colours are not up to a theme file. Everything that sets a colour goes
through one colour validator.

### Meaning colours are locked

These colours are fixed meanings, not decoration:

| Meaning | Colour | Hue | Shape |
| :--- | :--- | :--- | :--- |
| Needs you | Amber | 40 | Speech bubble, always shown with the words "needs you" |
| Claude | Coral | 14 | Square |
| Codex | Violet | 245 | Diamond |

- A theme may shift their tone for its ground, but never their hue family.
- An accent whose hue is less than 24° from any of the three is refused. Exactly 24° passes. The hue is the HSL hue,
  measured the short way round the circle. The bands of coral and amber overlap, so the allowed hues are 64° to 221°
  and 269° to 350°. A colour whose three channels are within 16 of each other has no hue and is exempt.
- An accent that lacks contrast is corrected automatically, then checked against the hue rule again.
- The lock is the owner's decision. It lives in three places only: the hue block of the validator, the `locked`
  section of `themes.json` and the read-only row in Settings.

### Contrast

Every ink is checked on every surface where it appears: 26 required pairs. Plain inks need 4.5:1 on the background and
the three surfaces. The three meaning inks need 4.5:1 on the same four. The accent needs 3:1 on the background, and its
ink needs 4.5:1 on the accent. The plain inks also need 4.5:1 on the frame, where the title bar and the sidebar
draw. A look with any failing pair is never applied, and the preview names the pair. For an
accent the user picks, the accent ink is pure black or pure white, whichever reads better. The factory looks keep their
own inks.

### Type

- **Text fonts:** Archivo, IBM Plex Sans and Atkinson Hyperlegible. The interface font belongs to each theme.
- **Heading fonts:** narrow (Archivo at 80% width, from Dark Room), book (Newsreader, from Workshop), poster (Barlow
  Condensed, from Workbench), or the text font.
- **Others:** Bodoni Moda for Cobalt, IBM Plex Mono, and Geist Mono for keyboard shortcuts.
- Fonts come from Fontsource, latin and latin-ext subsets only. The webview downloads a font file only when a face is
  used. No other font family ships.
- Every interface size is the base text size plus or minus a few pixels. Reading text is 1.5 px larger than the base.
- Display headings are clamped to `9vw`, so an 86 px heading fits small windows.

### Motion

- Chrome uses the `--motion-*` tokens in `ui/styles.css`: 150 ms feedback, 180 ms exits, theme-controlled 200–240 ms
  entrances and a 220 ms theme cross-fade. No chrome animation, including its stagger, exceeds 300 ms. The enter curve
  has a soft spring-like settle; exits ease in. Travel is 24 px for the running column, up to 6 px for content and a
  0.97 scale for cards and menus. Cards use a 2 px arrival blur; overlays use an 8 px static backdrop blur.
- New items rise up to 6 px over 200 to 240 ms; exits take 180 ms. Buttons respond in 140 to 240 ms. The "running" dot pulses every 1.6 to 2 s.
  Workshop only fades and never moves anything vertically.
- Frequency decides whether something moves at all. Anything the user does dozens of times a minute (a keystroke, a
  shortcut, a tab or row switch, a row hover) changes instantly or with a 150 ms fade of colour or opacity at most;
  expressive motion is for rare moments (first run, a finished task, an empty state). Motion is never the only
  signal: every animated change also leaves a static cue (colour, icon or label).
- Reduced motion and no motion both remove every animation and transition. A system that asks for less motion
  caps the user's level, and a change in the system setting changes only the level.
- The running column slides and fades; its thread uses a short measured width transition so glyphs never stretch.
  The stage is never cloned. During travel, stage shadows and rounded clipping are omitted to avoid expensive raster
  effects; they return at rest. Offscreen messages and markdown blocks use native content visibility with intrinsic
  size estimates. Panel content staggers by 18 ms, capped at 48 ms. Surviving list rows
  use FLIP. The composer and native disclosures retain short measured height changes; reduced motion is instant.
  Window resizing cancels travel and reflows instantly so content stays inside the new viewport.
  Exit copies are inert, isolated in a shadow tree and removed when their animation finishes. At most 12 visible copies
  per list exit together; no component or process is kept alive for an exit. Native dropdown pickers animate where the
  webview supports customizable selects, with the usual native control as the fallback. Theme changes use native
  View Transitions when available and apply immediately otherwise; no dependency or retained screen is needed.

### Effects
- Effects appear only on the new conversation screen, never behind text the user reads, and they stop when the window
  is hidden or another app has the focus.
  - **Dark Room:** a static projector beam, a glow that breathes every 7 s and 72 dust grains (CSS shadows) that fade
    out near any text block. A timer sets the opacity of the glow and the three dust layers ten times a second, which
    the compositor alone applies; as CSS animations they made a 240 Hz screen composite the stage on every frame.
  - **Workshop:** a loom at the bottom right whose shuttle crosses every 9 s, and paper grain.
  - **Workbench:** a flow line at the top right reading "Your idea → Claude and Codex → Done, with proof". It names only
    the vendors that are connected.
- A new project session in Dark Room shows a black hole drawn in monospace characters on one canvas (ui/Hole.tsx,
  --amb-hole and --hole-colors): a huge shadow cut by the right edge, an amber disk sweeping in from the lower left
  with fibres that turn faster near the hole, the far side of the disk bent over the top as a cool arc, a photon ring, a
  dim glow, a small planet below it and stars. Its size follows the room right of the text; words stay clear and the
  scene dims within 240 px of them.
  Ten times a second it works out again only the cells the hole can reach and repaints those that changed; stars
  twinkle every 700 ms and now and then one shoots across. A click on an empty spot sends a ring of the accent for
  1.3 s. It stops when the window is hidden or unfocused; reduced or no motion draws one still frame and no ripple.
  It opens with an entrance of about 2.8 s, once per new session, only with full motion in a shown, focused window:
  Curioso rises from behind his ledge and leans toward the hole, a spark leaves his eyes and flies a curve into the
  shadow, and the scene lights up from there outward, each character scrambling in the accent for 240 ms before it
  settles. Losing focus or hiding the window ends it at once.
- The texture switch controls grain and the dot grid. The glow switch controls the focus light and the amber light,
  which exist only in Dark Room and Cobalt; the other themes use borders.

### How a look reaches the screen

- `src-tauri/src/themes.json` is the single source of tokens, and `src-tauri/src/look.rs` is the only code that computes
  a colour. It applies the theme, the user's choices and the colour validator, checks the 26 pairs, and returns the CSS
  variables, the motion level, the effect switches and any notice. The screen never computes a colour.
- `state()` returns the look of the saved appearance, and `main.tsx` writes it on `<html>` before the first frame. The
  window background is the look's frame colour, so the first paint already matches.
- A look is a draft in Settings (see "Settings" below). "Use this look" calls `save_settings`, which answers with the
  settings as stored and the look computed from them, and the page wears that look only after the save succeeded. A
  refused look (a hue too close to a meaning colour, a failing pair) or a failed write changes nothing, keeps the draft
  and shows its sentence.
- A saved appearance that no longer parses, such as the old Mono, Studio and Cockpit maps, falls back to Dark Room.

## Identity

- **The symbol** is "Curioso": a lime creature with one lopsided, rounded shape, two pill-shaped eyes and a notch on
  the right, so it looks as if it is peeking. It is meant to be recognized, not to tell a story.
- **The app icon** is a single dark icon in every Windows mode: a lime #D6DC48 figure on a near-black #141A11 tile, with
  no border and a corner radius of about 22.6%. Hand-tuned 24 px and 16 px versions keep both eyes readable. On a dark
  taskbar the tile disappears and the lime figure carries the icon.
- **Inside the app** the mark and the lockup always use the ink colour, never lime, because lime sits too close to
  amber for the colour validator. The titlebar shows the lockup in every theme.
- **Empty states and the tutorial** use Curioso, static, for example peeking into an empty screen. The new conversation
  screen has none. He moves only in the entrance of a new project session in Dark Room (below). He is 64 px, in the third ink, peeking over a hairline: only his top 44 px
  show, eyes included (`Peek` in `ui/Mark.tsx`). Today he is in the two Projects empty states.
- **The wordmark** is "Faber" in Rubik Bold, converted to outlines. No font file ships.

## Brand

The sources are in `docs/brand/`: `mark.svg`, `icon-dark.svg`, `px24-dark.svg`, `px16-dark.svg` and `lockup.svg`. The
lime-tile variants were left out on purpose. `ui/Mark.tsx` holds the same paths inline, so a change to a source file
must be copied there.

**Wordmark.** "Faber" in Rubik Bold (SIL Open Font License), converted to outlines inside `lockup.svg`; no font file ships.

1. In a scratch folder outside the repo, `npm i @fontsource/rubik opentype.js`. Nothing is added to `package.json`.
2. Take the glyphs F, a, b, e, r from `rubik-latin-700-normal.woff` and place them by hand, because opentype.js cannot
   read the font's substitution tables: advance widths, plus a kern of -21 font units between F and a (the only pair the
   font moves; read with `measureText` in Chrome).
3. Scale so the cap height (700 units) is 64 / 1.65 = 38.79 units, put the baseline at y = 58.05, and start the F 24/130
   of a cap height (7.16 units) after the mark's right edge, 65.5. Rubik's x-height is 0.743 of its cap height; the board
   measured 0.75.
4. Write the outlines as a second path next to the mark's path. The view box is 0 0 226.93 64.

**App icons** (`src-tauri/icons/`).

1. `npx tauri icon docs/brand/icon-dark.svg -o <scratch folder>` makes the PNG, ICNS and ICO set. Keep `32x32.png`,
   `128x128.png`, `128x128@2x.png`, `icon.png` and `icon.icns`. Drop the Android, iOS and Windows Store files: Faber does
   not build for them.
2. Render `px16-dark.svg` and `px24-dark.svg` 1:1 in headless Chrome (device scale 1, transparent page, screenshot of the
   top-left 16 and 24 px) and save them as `16x16.png` and `24x24.png`. Clear the pixels whose alpha is 2 or less.
3. `icon.ico` is the scratch folder's ICO with its 16 and 24 px frames replaced by those two PNG files, byte for byte (its
   frames are PNG). The six sizes are 16, 24, 32, 48, 64 and 256.
4. `tray-16.png` and `tray-24.png` are copies of `16x16.png` and `24x24.png`, and `tray-32.png` is a copy of `32x32.png`.
   The tray (`src-tauri/src/tray.rs`) picks one by display scale: the smallest of the three that is not smaller than 16 px
   times the scale. It draws the amber badge of B6 on that picture at run time. If a picture fails to decode, the tray
   uses the window icon, which Tauri takes from the largest frame of `icon.ico`.
5. `src-tauri/tests/icons.rs` checks the sizes and that the 16 and 24 px frames equal their PNG files.

## Layout

The window has a 36 px titlebar across the full width, a 248 px sidebar, a stage with a 52 px topbar, a reading column of
at most 660 px centred in the stage, and a 352 px running column on the right. The window can be as narrow as 720 px,
so the layout changes with its width:

| Window width | Sidebar | Running column | Reading column |
| :--- | :--- | :--- | :--- |
| 1,100 px and wider | 248 px, beside the stage | 352 px when open, beside the stage | Shrinks with the stage, 660 px at most. The stage is never narrower than 500 px |
| 900 to 1,099 px | 248 px, beside the stage | When open, an overlay on the stage | Up to 660 px |
| 720 to 899 px | Hidden. A button in the topbar opens it as an overlay | When open, an overlay on the stage | Up to 660 px |

Text size up to 18 px must not push the page past the window width at any of these widths: content wraps or scrolls
inside its own area.

### Titlebar and topbar

- The titlebar has the lockup and the window controls. Dragging and double-click maximize keep working, and macOS keeps
  its native buttons.
- The topbar has a crumb ("Chat › title" or "Project › title"), a read-only model chip for the open conversation, and
  the run button. With nothing running the button reads "Nothing running". Otherwise it reads "{n} running". When an
  item needs the user it turns amber, shows the speech bubble and reads "{n} running · {m} needs you". The words are
  always visible.

### Sidebar

- At the top: "New conversation" (Ctrl N), then the two areas, Chat and Projects, the latter with a count.
- Each project expands to show its conversations, and has a "+". When items in a project need the user, its tail shows
  the amber speech bubble and "{m} needs you". Amber always comes with these words, never with a number alone. When
  something only runs, the tail shows a pulsing dot.
- Chats are grouped as Today, This week and Earlier. Deleting a conversation asks for confirmation.
- The footer has Settings and one line per connected vendor: its shape, its name and the used share of its tightest
  quota window. It opens Settings > Usage limits. A vendor with no reading yet, or only an expired one, shows "—". The
  footer shows no user name.
- Search comes in Phase 2. The percentage and "stopped" tails come with the progress view in Phase 3.

### The new conversation screen

- A greeting, "Where do we start?", with no name.
- Four hand-written suggestions per language. Clicking one fills the composer and moves focus there. It sends nothing.
- "Pick up where you left off" lists the three most recently updated conversations.
- The composer keeps its model picker, "AI: choose for me".
- Auto chooses both the model and its effort. A manual model adds an adjacent low, medium or high effort choice,
  defaulting to medium (R102). Each conversation remembers its choice. Changes apply to the next message only;
  the composer says so, and each answer keeps the effort actually used. Both CLI drivers always receive that effort.
- The ambient effect of the theme runs here.
- With no AI connected, the greeting, the suggestions and the list give way to an empty state with Curioso and a button
  that opens account management. The composer stays on screen but cannot send.

### Running column

- One block per project, plus one "Chat" block. Each row shows the vendor's shape (a neutral dot while Auto is still
  choosing the model), the conversation title, the state in words ("Working", "Needs you" or "Choosing the model") and
  the time since the turn started.
- An item that needs the user shows the permission card, and the user can answer in place with Allow, Always or Deny.
- Clicking a row opens that conversation, setting the area, the project and the conversation together.
- The column is closed when the window opens. It opens by itself once for each question it has not yet shown, 1.5 s
  after the last keystroke in any text field, including questions already waiting at launch. If the user closes it
  while questions are waiting, those questions never reopen it. Only the button stays amber.
- Items are grouped by project. Projects that have something needing the user come first, and inside a project the
  amber item comes first. The order is stable, so rows do not jump.
- With no items it shows an empty state with Curioso.

### Settings

- Settings replaces the shell, as before, with a list of five sections on the left: General, Appearance, Connected
  accounts, Usage limits and Privacy. Ctrl comma opens it on General, the quota of a vendor in the sidebar opens it on
  Usage limits, and Esc or Back closes it.
- General has the language, name, CLI hooks switch and Permissions choice. "Don't ask" is the default for new and
  existing settings; "Ask before commands" preserves the previous behavior. Choices save at once and stay at the saved
  value if the save fails. R107 applies the same default to Chat. Chat uses its chosen canonical folder
  (fixed before the first message), or private per-conversation scratch; deletion/draft cleanup never removes a chosen
  folder. General explains the rule and offers the folder choice.
- R121 (it replaces the next item for the two choices on screen): "Don't ask" is full access. No approval cards, commands
  run without Faber's sandbox and with network, WebFetch and WebSearch work, any folder can be the working folder, and the
  person's own MCP servers, plugins and hooks stay on, as in the CLI itself. Claude Code runs in bypass mode with Read and
  Edit deny rules for the credential folders (`.claude`, `.codex`, `.gemini`, `.ssh` and the other key files, and where a
  documented variable moved them), which apply in bypass mode to its file tools but not to arbitrary commands. Codex runs with
  `danger-full-access` and `never` approvals and has no way to block reads of the credential folders in that mode (see the
  R121 section of [the R97 security review](security-r97-review.md)). Ask names its modes (Claude `default`, Codex
  `untrusted`) and carries the same credential denies and the same refusal of forwarded credential requests; it asks except for
  what the person pre-allowed in the tool's own settings. The system and credential folders are refused as working folders, judged
  by their resolved path (R124, R124b), and Faber's own requests never run the person's hooks. A CLI and everything it started
  end together (a Windows job object made before the CLI runs, failing closed; R124b, R124c). Settings shows the
  details; the thread shows one calm line with a link to them. The isolated mode below is `Permissions::Isolated`: Faber's
  own model list, router, internal checks and self-development roles use it, and the person cannot pick it.
- No prompt can block a full-access turn. Where the installed Claude Code declares `--permission-prompts` as an option line in a successful `claude --help` (read once
  at start, and again when a launch fails with "unknown option", which also drops the newer flags at once; present on 2.1.296), full access starts it with `--permission-mode bypassPermissions --permission-prompts none` and no host prompt
  tool: a request that would have prompted is denied at once, listed in the turn's `permission_denials`, and kept on the reply as a
  parked request, as safe metadata only (tool, a kind, a file path or a page host, the time; never a command or any input, which
  could hold a token), at most 20 and 4 KB per turn with the rest counted ("+N more"). The row of the running list says "Needs you"
  with a card that has no buttons (the CLI is past it, and the work was not done) until the next message. The tray notifies once
  per turn ("N requests need you"), with a one-minute cooldown across turns (R92). It is never allowed on its own, and a request on a
  credential folder is not parked at all: the deny rules refuse it for good. A CLI without the flag, Ask and the isolated mode start
  as before. Faber's own Haiku workers and roles (the isolated mode and side questions) also start with `--autocompact 100k`, under the
  100K step above which Haiku costs more, where the CLI lists that flag; every other launch keeps the CLI default.
- R97/R107 (now the isolated mode only): bypass permissions with native deny rules for `.git` edits, credential directories
  and key files; Codex uses `never` approvals with a workspace-only permission profile and sandbox network access disabled.
  Native deny rules [apply even in bypass mode](https://code.claude.com/docs/en/permission-modes#available-modes), but
  [command patterns are not an OS sandbox](https://code.claude.com/docs/en/permissions#what-a-bash-rule-doesnt-match).
  Explicit ask rules [still prompt in bypass mode](https://code.claude.com/docs/en/permission-modes#actions-no-mode-auto-approves)
  and reach Faber's automatic policy over stdio. It gates file reads/writes, read-only git and literal local deletions, resolving junctions and refusing
  external or `.git` writes. Build/test tools and project entrypoints run through the installed native Codex sandbox
  with the same profile, never through an unsandboxed fallback. Native Windows requires MXC support. Unsupported shell
  syntax and arbitrary programs are declined without a card. Secret reads
  are denied while environment templates and internal `.env` creation work. Git transports and GitHub authentication
  are disabled in child commands. Both areas disclose the mode; tightening it stops current turns.
  Don't ask loads only user settings to keep project env/hooks out of the host launcher. Chat also uses safe mode,
  strict MCP configuration and its fixed tool list; Ask Chat retains restricted mode. Read-only shell commands and
  deletions use the sandbox too; file tools retain their canonical path checks, including network-link guards.
  Host file decisions, CLI startup customizations and external integrations are separate from command isolation;
  see the verified findings and limits in [the R97 security review](security-r97-review.md).
- Appearance is a draft. The six looks are small live renders; the controls (accent with a hex field, background, the two
  fonts, text size, density, corners, motion) and the three effect switches change the draft, and the preview panel on
  the right wears the answer of `preview_look` with the measured contrast of text, faint text and the accent. The panel
  says "Not applied yet" until "Use this look", which sits in a bar that stays in view. "Back to default" returns the
  draft to Dark Room. The three meaning colours are shown read-only, with their shapes and words.
- A refused accent, or a look with a failing pair, shows its sentence in the bar and "Use this look" stays as it was; a
  refused accent puts the draft back to the last one the shell accepted.
- An account counts as connected only when its CLI says someone is signed in; an installed CLI that says nobody is, or
  that cannot answer, is shown as such. The check runs each time the section opens.
- The preview draws the effect of the new conversation (a still picture of the beam, the loom or the line) and the two
  lights, so each of the three switches shows what it does before "Use this look".
- A theme file is written beside its destination and renamed over it only when the whole write worked, so a full disk
  leaves an old file as it was. A failed read or write shows words of Faber's own, never the system's.
- "Manage accounts" opens the account cards alone. Closing them (or Esc) brings Settings back on the same section, with
  the draft that was not applied.
- Usage limits lists, per connected vendor, every window it has reported with its used share and reset time. An old
  reading, or none, shows "—".
- Privacy has three statements: telemetry is off, history stays on this PC, and Faber never reads the CLI logins.

### First run and tutorial

- The first run has three steps in a window with no sidebar: connected accounts, appearance, first conversation. A stepper
  at the top shows the place. Each step is a modal dialog: its heading takes focus when it opens, and Esc and the
  shortcuts of the shell (Ctrl N, Ctrl comma, Ctrl period) do nothing until a button is used.
- Step 1 is the account cards, with Continue and Skip; both go on to step 2. Opened later, from the no-AI state or from
  Settings, the same cards have no stepper and only a Close button. They save `setup` and nothing else, so reconnecting an
  account never restarts the first run or the tutorial.
- Step 2 shows the three themes side by side, each a small live render drawn with its own tokens. A choice changes the whole
  screen at once and saves nothing. Under the cards: text size (three "Aa" sizes), density, motion, and "Do you already use
  AI?" (Never, A lot). Continue saves the appearance, the answer and `tutorialDone: false` in one call. Back keeps the
  choices. An install saved by an older version, whose appearance has no `base`, sees step 2 once.
- The tutorial starts at the new conversation, with an AI connected, while `tutorialDone` is false. A beginner gets three
  cards (Chat, Projects, Follow along); someone who answered "A lot" gets two (Auto, the running list); no answer counts as
  a beginner. A card has Curioso, "n of total", a title, one or two sentences, and Next (Got it on the last) and Skip.
  A ring and a pointer mark the real element the card is about (`data-tour`). When that element is missing or hidden, such
  as the Projects button below 900 px, the card is centred.
- The tutorial is modal: the shell is inert, focus starts on Next and stays on the card, and Esc skips. Finishing or
  skipping saves `tutorialDone: true` and gives focus back to where it was, or to the composer. It never reopens. If the
  save fails it still ends for that window and may show once more at the next launch.

## Rules for every screen

- Every text on screen lives in `ui/locales/`, written by hand in each language.
- Choice buttons use `aria-pressed`, or native radio groups. Switches are native checkboxes. "Open file" is a real
  button. Theme thumbnails are `aria-hidden`, and each theme card is named by its name and its description.
- Areas whose content can grow scroll; they never cut content off.
- Every overlay (the first-run steps, the tutorial cards, the narrow-window sidebar, the overlay running column) says
  whether it is modal. A modal overlay moves focus into itself when it opens and gives focus back when it closes. Esc
  closes it, except where a screen says otherwise. The one non-modal case is the running column when it opens by
  itself: it moves no focus at any width, nothing behind it becomes inert, and it ends above the composer so the
  composer is never covered. Opened by the user, the overlay is modal.

## Screen tests

Faber's own test runner (`cargo t`, the `testkit/` crate) tests the screen in two ways, both against a hidden test build
of the app. `FABER_HIDDEN=1` keeps the window offscreen with its own browser profile, so it never takes the user's screen.

- **Checks** (`tests/ui/*.check.ts`) test the screen's own logic. The test build opens `index.html?checks`, runs them when
  the page loads, writes the result to the file named by `FABER_CHECKS_OUT` and exits. No DevTools port is involved, so
  they run on Windows, macOS and Linux (Linux under `xvfb-run -a`).
- **Scenarios** (`testkit/src/screen/`, written in Rust) drive the real app like a person: they read the page, click and
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

Every launch is sandboxed: a temporary home, an empty `PATH`, no provider variables, a dead proxy and no real CLI (a fake
CLI replays the recordings in `tests/fixtures/`). Each scenario is audited afterwards for leaked processes, writes outside
the sandbox and a changed environment. The checks and the hooks that report them exist only in the test build
(`--features test-hooks`, `vite build --mode test`); the production app and its `dist/` contain none of them. CI runs
five jobs at once: lints with the `rust` and `ui` suites on Windows and macOS, and the `screen` suite on Windows in
three parts (`cargo t --suite screen --shard 2/3`). The runner deals the scenarios so the parts take about the same
time: the slowest first, each to the lightest part so far, using the CI seconds of the slowest few
(`testkit/src/screen/mod.rs`, `SLOW`); any other counts as 5 s. CI keeps the Windows screenshots as artifacts. Anything outside the page (tray,
notifications, installer, updates) is checked through the operating system's accessibility layer and file versions.

Test launches also pass the browser engine switches that turn off crash reports and SmartScreen address lookups, and the
sandbox removes every `WEBVIEW2_*` override from the environment. That limits what a test sends; it is not a firewall.
Nothing blocks the network at the operating system, and WebView2 has no documented switch for crash uploads, so a request
that bypasses the switches is not seen. The audit sees only requests that reach the fake release server.
Known limit: the loopback-only rule is not enforced. Test processes get dead proxy variables and the switches above, but
nothing stops a program from opening its own connection; blocking outbound traffic for test processes is deferred.

### Build folder budget

Each worktree builds into `target/<name>` of the main checkout (`faber-wt-projhome` into `target/projhome`), and Cargo
never deletes old artifacts. The runner (`testkit/src/sweep.rs`, R106) sweeps that `target` before choosing the suites, at
most once an hour (the `target/.faber-sweep` stamp), and prints a line only when it freed space:

1. A `target/<name>` folder with a `CACHEDIR.TAG` whose worktree is gone (no `git worktree list` entry named `<name>` or
   `*-<name>`) and that nothing used for a day is removed whole.
2. In every build profile (a folder with `.fingerprint`, nested ones included), a file in `deps/` that a newer build of
   the same crate and kind replaced more than a day ago is removed, and so is an `incremental/` folder a newer one of the
   same crate replaced. A crate's newest build stays however old it is. Measured on 2026-10-14, `debug/incremental` held
   52 folders for `faber_engine` alone, all but one dead.
3. If `target` is still above 60 GB, whole worktree folders are removed, the least recently used first, until it fits.
   A folder used in the last six hours belongs to a lane still at work and stays (a cold rebuild costs it ten minutes).
   The main checkout's own folders (`debug`, `release`, `faber-test`) and this run's own folder are never removed whole;
   if they alone exceed the cap, the runner says so.

A folder is in use while Cargo holds one of its lock files (`.cargo-lock`, `.cargo-build-lock`, `.cargo-artifact-lock`)
or, on Windows, while a program in one of its profiles runs (Windows refuses to open it for writing). In use, it is
neither swept nor removed. The sweep holds the profile's locks while it deletes, so a build that starts meanwhile waits.
sccache (user-wide, outside the repository) makes the rebuild of a swept dependency cheap.

### Build slots

Many lanes run `cargo t` on one PC; each run peaks near 8 GB. Before building, a run takes one of three slots
(`testkit/src/slots.rs`): a locked file `target/.faber-slots/slot-<n>` of the main checkout, held until the run ends,
crash included. With all three taken it prints one waiting line and waits. `FABER_BUILD_SLOTS` sets another count for
another PC (R129, E3).

### UX sentinel

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
  `tests/ui/mojibake.check.ts` covers every locale and source string).
- **Findings and the gate.** A finding is the hash of kind, screen, surface and element. Only critical and high
  findings whose hash is not in `tests/ux/baseline.json` fail a run; medium ones are written down and become tasks. The
  file is a ratchet: it lists what is known, and a fix removes its line. Findings go to
  `target/faber-test/ux/<scenario>.json` and raw frame traces to `target/faber-test/ux/traces/`. Nothing records the
  person's screen.


## Auto quota pacing

Auto requires billing confirmed as a subscription before any user, router or summary request. The same quota gate
checks every dispatch and rechecks waiting requests; plan checks spend no quota. Unknown billing is denied. The shell
starts account watching before Auto, and the screen supplies the local UTC offset on start and each minute. Vendor-wide
windows pace the whole shared account against approximately one seventh of the weekly quota per day; family windows
restrict only that family. Paid-fallback evidence survives restart, and missing evidence remains possible. Approval for
paid credits is bound to one conversation, reply and vendor, is consumed by that dispatch, and cannot authorize an
auxiliary request or bypass unknown billing.

Quota readings never ask a model anything (see "Quota readings that stay current" below). `budget.json` stores bounded quota history, metered spans, learned whole-account shape, explained model weights and
own spend. User turns, routing and hand-over summaries count in Faber's own API-equivalent USD, including cache
reads and writes. Percentage changes and shared-account credits never become Faber's own dollars. Codex prices each
request's token delta at its context tier; a multi-request turn does not inherit a long-context fee just from its sum.
Incomplete meters show a lower bound and cannot teach weights or plan size. Idle account movement is flagged as foreign,
even below the two-point resolution of the numerical outside-use estimate. Own-cost learning accepts only explained,
non-overlapping stretches; the weekly shape continues to describe the whole account. The spec's R89/R90 sections are
in `docs/superpowers/specs/2026-10-11-auto-quota-pacing-design.md`.

When included quota runs out, Auto tries another eligible profile once, carrying the conversation and partial reply;
long histories use a bounded, tool-free summary. It waits only for a usable reset within six hours, with guarded
resumption, Stop and shutdown cancellation. A pinned vendor asks before moving. The composer retains an unsent request
and its attachments without overwriting newer input; explicit resumption clears only the matching restored draft.
Settings separates whole-account bars from Faber's own spend and explains foreign use, incomplete meters and billing.

### Quota readings that stay current

Both vendors' counters are read by commands that answer locally, so a reading costs no model tokens:

- Claude Code: `claude -p /usage --output-format json` (with Faber's usual isolation flags, no session saved). `/usage` is a local
  command; the result reports `num_turns` 0, zero tokens and zero cost, and Faber checks that on every reading: if a CLI
  version ever answered it with a model, the result is thrown away and `/usage` is never run again. Its text gives the session
  and weekly percentages; the reset times are printed in the PC's own time, which the screen's offset converts. A reset time
  within two minutes of the one already held is the same window (the stream prints seconds, `/usage` the minute).
- Codex: `account/rateLimits/read` on a short-lived `codex app-server` (handshake, the two reads, nothing else).
- During Faber's own requests both CLIs already send their windows with the stream (Claude's rate-limit events, Codex's
  rate-limit updates); those go through the same recording.

When it is read (`engine/src/cadence.rs`, tested with a fake clock): on start, then every 15 minutes whether the window is
visible or hidden (the owner’s ceiling is a reading never more than 30 minutes old). Three things may ask for an earlier
reading: a request that ended, the window gaining focus, and the PC waking up (a loop that was not heard from for 90 seconds
slept). All three share ONE debounce per vendor: no reading sooner than 5 minutes after the last one for Claude Code (whose
reading starts a heavy Node process, about 12 CPU-seconds and 330 MB for a few seconds) and 1 minute for Codex (about 1
CPU-second), whatever caused the last one. The timer counts from the last reading too. A sleeping PC reads nothing.

Reads are short-lived, hidden (no console window, no shell), one at a time per vendor (a marker lifted by a guard, whichever way
the read ends), and a failure leaves the old reading in place. Windows creates them at idle priority
(`IDLE_PRIORITY_CLASS` in the creation flags, so not one instruction runs at normal priority); elsewhere there is no safe way to
lower the priority from Rust (it would need unsafe code, which Faber keeps to one Windows file), so they run at normal
priority. `/usage` is accepted only after a clean exit, `is_error` false, and every metering field present and well typed
(`num_turns`, the four token counts, the cost); a missing field is no reading, never zero, and any model work makes it `Spent`.
The reset times it prints are parsed defensively (ASCII am/pm only, minutes 0 to 59, hours 1 to 12, days 1 to 31, checked
arithmetic), because the text comes from a program Faber does not control.

The vendors count with a delay (docs below, "Quota lag, measured"). Faber therefore keeps, per vendor, its own requests (`Work`
in the ledger). A request is pending for a window from the moment it ends until that same window has been read after it ended plus
the lag: reading another window settles nothing, and time alone releases nothing, so when every refresh fails the floor stays up.
Pending requests are never dropped; past 100 the oldest are folded into one record that keeps their convertible points and the
latest end. While anything is pending, or a turn of the vendor is
running (for decisions as well as for the screen), the reading is marked "still counting" and no discount is believed on it.
Where the window is the one the conversion was measured on and the request’s model has a measured points-per-dollar (the
learned weights), the unreported share is added as a separate estimate (cost x points per dollar, minus what the reading has
already risen since the work began, taken off only when all the pending work is convertible). Work that cannot be converted adds
nothing to the number but never takes the known part away. No conversion, no number. Decisions use the reading plus that
estimate as a floor. The floor is only computed for the primary window (the shortest vendor-wide one: Claude’s five hours,
Codex’s week); other windows stay as read, so admission on them can run slightly past what a settled reading would allow.

"Running low" is said only when a vendor-wide window is past 90%: under one day of the daily line (R88: 100% / 7 = 14.3%) is
left. Being ahead of the line is said as "being used fast this week" and never as running out, in the answer’s note and in
Settings alike.

### Quota lag, measured

Measured on 15/10/2026 with a 3-minute poller over 90 minutes (both zero-token sources) plus one deliberate Codex run; the
full table and method are in the maintainer notes (`pesquisa/quota-lag.md`).

| Source | Measured lag | Window Faber assumes |
| --- | --- | ---: |
| Claude Code `/usage` | session counter within one poll (3 min) of work ending; weekly counter seen rising 4.1 min after the last request | 10 min (`LAG_CLAUDE`) |
| Codex `account/rateLimits/read` | weekly counter rose 45 to 46 and was seen 29 s after a turn ended | 5 min (`LAG_CODEX`) |

Both are bounds, not physics: the percentages are whole numbers and the poll was every 3 minutes. The vendors do not make the
owner’s 30-minute ceiling impossible. Slow movement seen while Faber is idle is more likely use from the same shared account
elsewhere than a late counter. Points per dollar are not measured for every model; where they are unknown only the
"still counting" marker shows.

### Synthetic monthly acceptance

`engine/tests/simulation.rs` runs four weeks with 200 deterministic seeds, busy weekday usage, shared-account use and
a burst. Both modes use the same gate and synthetic capacity: 100 cost units for the smaller vendor, 400 for the larger.
The simulation provides ideal fresh readings; it does not model real account latency or establish a live guarantee.
The full test is ignored in ordinary runs and was run explicitly with
`cargo test --release -p faber-engine --test simulation -- --ignored --nocapture` on 2026-10-12.

| Measure | Pacing on | Pacing off |
| --- | ---: | ---: |
| Mean exhausted hours per four-week run | 0.135 | 0.900 |
| Seeds with any vendor exhausted in four weeks | 2.0% | 14.5% |
| Median unused share of the larger quota at reset | 2.5% | 0.9% |
| Mean profile IQ + agent score on the tasks that matter | 81.42 | 80.58 |
| Dropped task share | 0.597% | 0.151% |
| Mean dropped tasks per four-week run | 65.5 | 16.6 |

No budget constants were tuned for this run. The four-seed smoke uses `[0, 1, 2, 12]`; seed 12 adds an unpaced-exhaustion
case because the original first four seeds all tied at zero exhausted hours. The smoke asserts strictly fewer exhausted
hours, fewer than 1% dropped tasks, no worse quality, fewer than 5% exhausted seeds, and less than 25% unused larger quota.
It passed with 0.000 versus 1.500 exhausted hours and 0.844% versus 0.325% drops. The full 200-seed run passed all the same
acceptance limits.

### Cache and router, measured on 13/10/2026

`engine/examples/cache_probe.rs` ran against the real CLIs on a Claude Max subscription (Sonnet 5.5) and a ChatGPT
subscription (GPT-6 Luna). Every question carries about 3,800 new tokens, so a second turn that reads the whole earlier
prefix still shows about 72%: the new question is written, not read. The table now has a `1h written` column.

| Scenario | Second turn: read / written | Reading |
| :--- | :--- | :--- |
| Claude, three turns in one process | 10,876 / 4,296 | the baseline: the whole prefix is read |
| Claude, resume at once, same effort | 10,876 / 4,299 | a restart with `--resume` loses nothing |
| Claude, medium, then resume at high (twice) | 10,876 / 4,296 and 4,295 | **an effort change keeps the cache** |
| Claude, resume after 240 s and 360 s | 10,876 / 4,295 and 4,298 | the 120 s idle close costs nothing |
| Claude, resume after 3,700 s | 0 / 15,171 | the cache lives one hour, then everything is written again |
| Codex, three turns on one thread | read 43%, 34%, 86% | Codex's own cache varies a lot between turns |
| Codex, low then medium on one thread (twice) | read 19% and 34% | no loss beyond that variance |
| Codex, a turn after 6 min and after 35 min | read 84% both | still warm after half an hour |

On a subscription, Claude writes every cache entry with the one-hour lifetime, so closing an idle process after 120 s
and resuming it inside the hour is free, and so is a change of effort through `--resume`. Changing effort before the
next message therefore needs no deferral to a natural break. A change of model still starts a new cache.

The result of every Claude turn names which bucket its cache writes went into (`usage.cache_creation.ephemeral_1h_input_tokens` and
`ephemeral_5m_input_tokens`). Faber keeps "warm until" on the reply (`Message::cache_warm_until`: an hour for the 1-hour bucket, five
minutes for the 5-minute one, nothing when neither is named) and the latest reply shows it quietly in its usage line, hidden when
unknown or cold. The lifetime counts from the start of the last request (its assistant message start in the stream, less a 5 s margin),
not from the result; a turn whose start was not seen shows nothing. It is display only: Auto does not route by it, because a change of model starts a new cache anyway. Codex names no
lifetime, so it shows nothing.

`engine/examples/route_eval.rs` sends 30 hand-labelled requests (kind, difficulty and the new `check`) to each router,
one fresh session each, the way Auto asks:

| Router | Valid JSON | Kind | Difficulty | Check | Median / slowest |
| :--- | :--- | :--- | :--- | :--- | :--- |
| GPT-6 Luna, medium | 100% | 93% | 70% | 87% | 3.3 s / 9.1 s |
| GPT-6 Luna, low | 100% | 90% | 70% | 83% | 3.3 s / 4.5 s |
| Haiku 5.5, medium | 100% | 100% | 70% | 73% | 9.6 s / 28.7 s |
| Haiku 5.5, low | 100% | 100% | 70% | 77% | 9.5 s / 19.7 s |

Low effort made neither router faster at the median and Luna slightly less accurate, so the routers stay at medium.
The labels are one reader's, so a disagreement is a question, not a proven mistake. By the labels, the requests the
routers called `blind` against the label were easy, of a kind the raise ignores, or already hard; the raise only moves
medium code, math and research.

### Measurement still requiring the owner

The manual `engine/examples/cache_probe.rs` now obtains one baseline before its measured pulses, retains each window's
latest reading, checks unchanged resets and plausible readings, closes immediately on overage, rejection or lost billing,
and applies the gate before each pulse with a sixty-second timeout. It prints own API-equivalent USD separately from
account movement, which is an upper bound on a pulse's quota effect because the accounts are shared. Its three tests
use fake events only, covering the denominator, invalid/reset/missing readings, duplicate windows and immediate stops.
The cache experiment ran on 13/10/2026 (above). The real CLI pulse memory peak has not been measured, and no pulse peak
is claimed.

Existing hidden-screen memory proofs passed: Task 11 measured 108-110 MB idle, 184 MB after six looks, and 141 MB for
1,000 messages. These measurements used the fake CLI and are not a real pulse memory measurement. With
`CARGO_TARGET_DIR=target/proofs`, testkit artifacts and screenshots live in `target/proofs/faber-test`; no junction is
needed. The locale keys, review disposition and implementation limits are listed in
`docs/superpowers/auto-quota-pacing-report.md`.

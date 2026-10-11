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

### Motion and effects

- No arrival or transition lasts longer than 400 ms.
- New items rise 6 px over 400 ms. Buttons respond in 140 to 240 ms. The "running" dot pulses every 1.6 to 2 s.
  Workshop only fades and never moves anything vertically.
- Reduced motion keeps only fades of up to 120 ms. No motion removes everything. A system that asks for less motion
  caps the user's level, and a change in the system setting changes only the level.
- Effects appear only on the new conversation screen, never behind text the user reads, and they stop when the window
  is hidden.
  - **Dark Room:** a static projector beam, a glow that breathes every 7 s and 72 dust grains drawn at 24 fps that fade
    out near any text block.
  - **Workshop:** a loom at the bottom right whose shuttle crosses every 9 s, and paper grain.
  - **Workbench:** a flow line at the top right reading "Your idea → Claude and Codex → Done, with proof". It names only
    the vendors that are connected.
- The texture switch controls grain and the dot grid. The glow switch controls the focus light and the amber light,
  which exist only in Dark Room and Cobalt; the other themes use borders.

### How a look reaches the screen

- `src-tauri/src/themes.json` is the single source of tokens, and `src-tauri/src/look.rs` is the only code that computes
  a colour. It applies the theme, the user's choices and the colour validator, checks the 26 pairs, and returns the CSS
  variables, the motion level, the effect switches and any notice. The screen never computes a colour.
- `state()` returns the look of the saved appearance, and `main.tsx` writes it on `<html>` before the first frame. The
  window background is the look's frame colour, so the first paint already matches.
- Settings applies a choice at once, but only after `save_settings` succeeded. That command answers with the settings as
  stored and the look computed from them. A refused look (a hue too close to a meaning colour, a failing pair) or a
  failed write changes nothing and shows its sentence. The draft and preview flow comes with the Settings rebuild.
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
  screen has none. Curioso does not animate yet.
- **The wordmark** is "Faber" in Rubik Bold, converted to outlines. No font file ships.

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

Every launch is sandboxed: a temporary home, an empty `PATH`, no provider variables, a dead proxy and no real CLI (a fake
CLI replays the recordings in `tests/fixtures/`). Each scenario is audited afterwards for leaked processes, writes outside
the sandbox and a changed environment. The checks and the hooks that report them exist only in the test build
(`--features test-hooks`, `vite build --mode test`); the production app and its `dist/` contain none of them. CI runs
`cargo t --all` on all three systems and keeps the Windows screenshots as artifacts. Anything outside the page (tray,
notifications, installer, updates) is checked through the operating system's accessibility layer and file versions.

Test launches also pass the browser engine switches that turn off crash reports and SmartScreen address lookups, and the
sandbox removes every `WEBVIEW2_*` override from the environment. That limits what a test sends; it is not a firewall.
Nothing blocks the network at the operating system, and WebView2 has no documented switch for crash uploads, so a request
that bypasses the switches is not seen. The audit sees only requests that reach the fake release server.
Known limit: the loopback-only rule is not enforced. Test processes get dead proxy variables and the switches above, but
nothing stops a program from opening its own connection; blocking outbound traffic for test processes is deferred.

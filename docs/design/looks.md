# Looks

Themes, presets, customization, colour, type, motion, the one grammar, idle cost and effects.

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
| Needs you | Amber | 40 | Speech bubble, always shown with the words "needs you" (inside a reply, a subagent's row says "waiting for you" or "seems stuck") |
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
- Every size is a token that follows the text-size setting: `--fs-xs` (small print and counts), `--fs-sm` (secondary
  lines and fields), `--fs-ui` (the interface), `--fs-read` (reading text), `--fs-lead`, and the heading and display
  tokens. A style never writes a pixel text size or `calc()` around one (a check reads the real stylesheet).
- Display headings are clamped to `9vw`, so an 86 px heading fits small windows.

### Motion

- Chrome has one timing family (see "One grammar"): `--dur-fast` feedback, `--dur-ui` entrances and the theme
  cross-fade, `--dur-exit` exits, `--dur-move` and `--dur-arrive` for travel, with the curves `--ease` (a soft
  spring-like settle), `--ease-in` (exits) and `--ease-spring`. No chrome animation, including its stagger, exceeds
  300 ms. Travel is 24 px for the running column, up to 6 px for content and a 0.97 scale for cards and menus. Cards use
  a 2 px arrival blur; only the scrim has a backdrop blur.
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

### One grammar

Every control, text size and timing comes from a short list of tokens in `ui/styles.css`, so two screens never differ by
accident. `tests/ui/grammar.check.ts` reads the real stylesheet and fails on a break.

- **Control heights:** `--h-sm` 28 px (chips, pills, the plus, the rename field, small icons), `--h-md` 32 px (buttons,
  selects, fields, the segmented control, the send button, sidebar rows) and `--h-lg` 40 px (the big button, the new
  conversation button, the bottom bars of overlays). No button, select, chip, input or segmented control has a height
  of its own. A row that scales with density adds `2px * var(--dens)` to a token.
- **Text sizes:** only the `--fs-*` tokens above; a size that is not a token is written in `em` of its parent.
- **Timing:** `--dur-fast`, `--dur-ui`, `--dur-exit` and the two travel durations come with the curves `--ease`,
  `--ease-io`, `--ease-in` and `--ease-spring`. A transition or animation never writes a literal duration or curve.
  Scenes that loop or play once on the home (the wake, the blinking caret, the weave and the drift of the effects) use
  the `--dur-wake`, `--dur-blink`, `--dur-pulse`, `--dur-weave`, `--dur-weave-slow` and `--dur-drift` tokens: they
  are not interface timing.
- **One text field:** `.field` is the text input and textarea of every screen outside the composer (first run, setup,
  Settings, the project's proof commands, the plan question, the rename of a row, the folder set name, a question's
  own answer). `.field.sm` is the 28 px one, `.field.hex` the colour code and `.field.search` the sidebar search, a
  box that holds an icon and an input. The composer and the edit box of a sent message sit inside their own bubbles.
- **Hover:** a row or a button changes its background or its colour on hover and never its brightness.
- **Blur:** a panel that is at least 94 % opaque draws no backdrop blur; only the scrim has one.

### Idle cost

- Nothing keeps a timer while the window is hidden or minimized: the one-minute clock (which also tells the engine the
  time zone) and the running column's 15 s tick are made by `everyWhileShown` (`ui/idle.ts`), end when the window hides
  and run once at once when it returns. The Dark Room dust reads its nodes once, not ten times a second.
- Animated lists share one `resize` listener and do not measure their children after a transition that only paints
  (colour, opacity, shadow). The thread and its column share one `ResizeObserver`. `tests/ui/lighter.check.ts` fails
  when one of these comes back.
- Fonts ship as woff2 only: a PostCSS plugin (`scripts/woff2-only.mjs`) drops the woff fallback that Fontsource lists,
  which the webview never reads.

### Quiet during games

While a game or another full-screen app is in front of the shown window (R191), the screen acts as if its window were
hidden. The shell answers `game_in_front` from the check the tray already makes before a notification
(`src-tauri/src/tray/native.rs`, `decide::game_in_front`): Windows names a full-screen, Direct3D, presentation or store
app, or another process's window covers its whole monitor (a borderless game). Faber's own window never counts.

- The screen asks (`ui/quiet.ts`, `watchGames`) only while its window is shown without the focus: once at the blur and
  every 5 s after. The focus back, or the window hiding, ends quiet at once, and an answer that lands later is dropped.
- Quiet counts as `windowHidden()` (`ui/appearance.ts`): idle life, the dust and the black hole stop, and
  `everyWhileShown` ends the clock and the running column's 15 s tick, running each once on return. `html.quiet-win`
  pauses every CSS animation, the live marks included, as `html.hidden-win` does.
- The engine's events wait (`createHold`), one per message, queue, quota window and running list (the higher revision
  wins), plus one `list` and one `usage`, so an hour of streaming keeps only the newest copy of each. They land in one
  batch on return: one repaint for the whole time away. The tray, its counts and its notifications do not wait, nor
  do the progress of an account install (`ui/Setup.tsx`) and of an update (`faber:update`), which the person started or
  watches and which end by themselves.
- Nothing opens a window, takes the focus or flashes the taskbar. macOS and Linux answer no until they have a probe (R92).
- `tests/ui/quiet.check.ts` fails when a game in front leaves a timer, an animation, a late answer or a second repaint.

### Effects
- Effects appear only on the two homes (the new conversation of Chat and the first screen of a project) and behind a new
  project session, never behind text the user reads. Every animation, including the composer's idle life, pauses while the
  window is hidden (`html.hidden-win` sets `animation-play-state: paused`); the effects also stop while another app has the
  focus (`html.away-win`), and they are gone with reduced or no motion.
- Slow ambient loops step on a shared 1 s beat (`steps()`) instead of drawing 60 frames a second: idle CPU follows the
  frames per second, not what changes in them. Dark Room's idle home costs about 10% of one core against 49% before.
- The Projects home is a place to work, like a terminal: it has no beam, dust or drifting light. Its loom runs slower
  and its flow line reads "Your goal". A look with no effect of its own gets a faint drifting light when the ambient
  switch is on.
- The empty composer idles (`ui/Ghost.tsx`): a blinking typing bar and a hint that changes every 8 s. It renders
  nothing while the field has text or focus, with reduced or no motion, or while the window is hidden; the plain
  placeholder stays underneath for assistive technology.
  - **Dark Room:** a static projector beam, a glow that breathes every 7 s and 72 dust grains (CSS shadows) that fade
    out near any text block. A timer sets the opacity of the glow and the three dust layers ten times a second, which
    the compositor alone applies; as CSS animations they made a 240 Hz screen composite the stage on every frame.
  - **Workshop:** a loom at the bottom right whose shuttle crosses every 9 s, and paper grain.
  - **Workbench:** a flow line at the top right reading "Your idea → Claude and Codex → Done, with proof". It names only
    the vendors that are connected.
- The first screen of a project (where a new session starts) in Dark Room also shows a black hole drawn in monospace
  characters on one canvas (ui/Hole.tsx, --amb-hole and --hole-colors): the whole shadow, centred in the room from 150 px right of the column with its arc clear of every edge (14% to 22% of the height), an amber disk sweeping in from the lower left that reaches 3.6 radii and fades from 2.6, so the scene keeps to the room beside the column instead of filling the stage,
  with fibres that turn faster near the hole, the far side of the disk bent over the top as a cool arc, a photon ring, a
  dim glow, a small planet below it and stars. It has no hard edge beside the column of words: behind the column it keeps 6% of its light, comes up to full
  from 40 px left of the column's edge to 240 px right of it and dims the hole a little more within 320 px, across the
  whole height, so words that scroll in the column always sit on the same veil. The veil follows the edge of the words, not only the window: when the column's words change (a list opens, a longer name shows) it is laid again, once a frame at most and only when that edge moved. The project screens show no beam of light. With less than 400 px of room it does not show or run.
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

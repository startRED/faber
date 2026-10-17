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
  show, eyes included (`Peek` in `ui/Mark.tsx`). He is in the one empty state (`ui/Empty.tsx`, R170): Curioso, a title in
  the heading size (`--fs-h2`, never the display size), one short line and at most one button, for no AI, no project and a
  project with nothing in it. The line never names a path through Settings, whose sections change names; the button goes there.
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
- The topbar has a crumb ("Chat › title" or "Project › title") and the run button. The model is not shown there: it is
  chosen in the composer and each answer names the one it used (R168). With nothing running the button reads "Nothing running" as quiet second-ink words with no outline or
  fill (styled from the absence of the live mark, `.run-btn:not(:has(.live))`); the outline comes back on hover,
  keyboard focus and while the column is open. Otherwise it reads "{n} running". When an
  item needs the user it turns amber, shows the speech bubble and reads "{n} running · {m} needs you". The words are
  always visible.

### Sidebar

- At the top: "New conversation" (Ctrl N), then the two areas, Chat and Projects, the latter with a count.
- The sidebar lists only the open area's items (R168): Chat lists the chats and no Projects block, not even an empty
  one; Projects lists the projects with their conversations and the add-folder control, and no chats. Rows keep their DOM
  when the list refreshes, so a refresh moves nothing the person is pointing at.
- Each project expands to show its conversations, and has a "+". When items in a project need the user, its tail shows
  the amber speech bubble and "{m} needs you". Amber always comes with these words, never with a number alone. When
  something only runs, the tail shows a pulsing dot.
- Chats are grouped as Today, This week and Earlier. Deleting a conversation asks for confirmation.
- The footer is one row, the person's name when they gave one and Settings, then one thin line per connected vendor: its
  shape, a 3 px bar and the used share of its tightest quota window, in the small size. Nothing on the line wraps. The
  window, the reset, how old the reading is, the not-yet-counted estimate and the vendor's name are its tooltip. While
  use is still being counted the share reads "≥ 44%" (the real share can only be higher), with the words in the tooltip
  (C010). The bar is amber only when the engine says the quota is nearly used. The line opens Settings > Accounts and limits.
  A vendor with no reading yet, or only an expired one, shows "—". The version is not on the sidebar: it is the
  tooltip of the Settings button.
- Search filters conversation titles and project names on screen; full-text search comes in Phase 2. The percentage and
  "stopped" tails come with the progress view in Phase 3.

### The new conversation screen

- One column, like the Projects home (C005): the greeting, the composer, the suggestions and what to pick up, top to
  bottom. No line under the greeting: the composer's placeholder already says to ask anything, and its permission chip
  shows whether Chat may change files.
- A greeting, "Where do we start?", ending with the person's name when they gave one.
- Three short hand-written suggestions per language, as chips under the composer: one row at the usual text size, the
  last chip wrapping only on a narrow window or a large text size. Clicking one fills the composer and moves focus there. It sends nothing. A hover changes only the chip's colour.
- "Pick up where you left off" lists up to three conversations of both areas under the suggestions: those that need the
  person first, then the most recently updated.
- The composer keeps its model picker, "AI: choose for me".
- Auto chooses both the model and its effort. A manual model adds an adjacent low, medium or high effort choice,
  defaulting to medium (R102). Each conversation remembers its choice. Changes apply to the next message only (the
  composer does not repeat it), and each answer keeps the effort actually used, in its header's tooltip. Both CLI drivers always receive that effort.
- The ambient effect of the theme runs here, and the empty composer idles with a typing bar and rotating hints.
- With no AI connected, the greeting, the suggestions and the list give way to the empty state (Curioso, "No AI connected",
  one line and the button "Connect", which opens account management). The composer stays on screen but cannot send. An open conversation with no AI connected
  shows no composer and no permission line.

### Answers

- The header is the vendor's mark and the model's name, nothing else (Auto shows its reason beside the name only while it is
  still choosing). The effort and Auto's reason are its tooltip. The pace line stays (C002). A wait for quota is told once,
  by the quota card, never also in the header or the red note.
- Under the answer, one line that never wraps, at a 660 px column or a 720 px window: three icon buttons from
  `ui/Glyphs.tsx` (copy, retry, open in terminal, each with a tooltip and an accessible name), a short "Retry stronger"
  when a step up is offered, then the cost in the faint data style. The tokens and how long the cache stays warm are the
  cost's tooltip. There is no model select under an answer: a retry (and the right-click Redo, which names the model)
  uses the composer's current choice.
- There is no permission sentence over a conversation: the mode is a Settings matter.
- While an answer streams the engine sends the whole message every 33 ms, so the text is drawn block by block
  (`ui/markdown.tsx`): a block whose source did not change keeps its DOM and only the open one is parsed and sanitised
  again. A tick that changed only the tools builds no text. Everything that reaches the page passes DOMPurify.

### Work log

- The work log and a helper's row share one disclosure style (`.disclosure`: a quiet card, a chevron, the hover).
- Each step is a quiet chip: a verb in the ink color ("Reading", "Running") and what it acts on in the mono face, cut
  in the middle. A tool Faber does not know keeps its own name; an MCP tool reads "Using a tool" with its server's name.

### Subagent steps

When the agent hands part of the work to a subagent (R161), the reply shows one row per helper between the work log and
the answer, in the work log's surface and type: a check, a cross or the live mark, the few words it was given in bold, and
on the right its kind, its state and its step count in the third ink. The row opens like every disclosure (`animateDetails`)
to the tool chips it ran and its report as Markdown; nothing else moves. Rows sit in a `minmax(0, 1fr)` grid, so a long
tool line ellipsizes instead of widening the reply. A helper that never finished reads "stopped" once the reply has
ended; one sent to the background keeps the live mark until its task notice arrives (a ring, never the square of a stop button); a refused one reads "declined" in
the third ink, never as a failure. Known limit: when a reply moves to another AI after a quota stop, its open tools,
helpers included, read as working again with the new reply.

A Codex subagent (`spawn_agent`, started only when the person asks for one) is the same row. Codex gives it a path instead of a description, so its title is the
path's last part with spaces (`/root/read_docs` reads "read docs"), and it has no kind. Its approvals show on the
conversation's own card, since the person cannot open the subagent; one that arrives after the reply ended is refused,
and Stop and closing the conversation reach the subagent too. What a subagent still does after its reply ended reaches
no reply, so its step reads "stopped". Codex counts each thread's usage on its own, so the
engine adds the subagent's tokens and cost to the reply that started it, and marks the reply's usage incomplete when a
subagent is still at work as it ends.

A subagent waiting on the person's answer reads "waiting for you" with its live mark in amber; the card under the rows is
that answer. One that seems stuck (R161 step 6, R174) is the conversation's other amber: the mark and the row's edge
turn amber, its state reads "seems stuck", and one line under the row says in plain words what the engine saw, with
"Look", which opens the row to what it ran, and "Tell it…", which focuses the conversation's box with "About “title”: " at
its start (Codex takes it into the running turn; Claude Code after it, or at once with "Send now"). The line arrives with
the normal entrance (`--dur-arrive`, none in reduced motion) and goes when the subagent moves again or the reply ends; the
words carry the meaning, the amber only repeats it. The signs are counted by the engine (`engine/src/stuck.rs`): the
same step six times in a row, the same step failing three times with no file changed between (twice and its retry), or
no step for 10 minutes (30 while a command runs, since builds say nothing until they end). The time a question waits for
the person never counts. Neither CLI lets Faber speak to the main agent while it waits for its subagent, so the main
agent's own chance is the retry the numbers leave it. Background subagents are not followed.

A helper from the other AI (Faber's helper tool, R161) is the same row: its title in bold and, as its kind, the AI that
ran it, so a Claude Code reply reads "Codex". Its commands show under it and its report is its final words. The person
never sees its approval cards: in a mode that asks, what the helper needed approval for is refused and named in its
report, and the agent does that step itself, where the card shows. Known limit: a conversation resumed from before the
helper tool existed keeps its Codex thread without it, since Codex takes dynamic tools only when a thread starts.

### Recent folders and environments

The "Open a project" screen (Projects with none open) and the Chat folder setting list the last five folders used, newest
first, one click each: a short name and the dimmed parent path. The engine judges each one when the list is read
(`recents` command, `engine/src/recents.rs`): a folder that is gone or refused by R124 shows dimmed with its reason and
cannot be picked, and a refused folder is never stored. Right-click on a row opens Faber's shared one-line menu, "Remove
from recents", which asks once before it forgets (C009). In the project area a "+" on each row builds a selection that
can be opened at once or saved as a named environment (up to 12 environments, 8 folders each; the same name replaces).
Starting an environment opens the folders that still exist as
projects. Faber has no worktree creation for projects, so an environment has no "own worktree" flag. Both lists live in
`recents.json` next to `settings.json`, written through a temp file and renamed.

### Running column

- With nothing running it says "Nothing running" once and has no footer sentence.
- One block per project, plus one "Chat" block. Each row shows the vendor's shape (a neutral dot while Auto is still
  choosing the model), the conversation title, the state in words ("Working", "Needs you" or "Choosing the model") and
  the time since the turn started.
- An item that needs the user shows the permission card, and the user can answer in place with Allow, Always or Deny.
  A question the agent asks (`AskUserQuestion`) shows instead as a question card, in the thread and in the column alike:
  each question in bold with its short header chip, its options as full-width rows to tap (amber ring when chosen,
  checkboxes when several are allowed), one line to write another answer, then Send answers (enabled once every
  question has an answer) and Skip. No motion beyond the card's arrival; option hovers do not animate under reduced motion.
- Clicking a row opens that conversation, setting the area, the project and the conversation together.
- The column is closed when the window opens. It opens by itself once for each question it has not yet shown, 1.5 s
  after the last keystroke in any text field, including questions already waiting at launch. If the user closes it
  while questions are waiting, those questions never reopen it. Only the button stays amber.
- Items are grouped by project. Projects that have something needing the user come first, and inside a project the
  amber item comes first. The order is stable, so rows do not jump.
- With no items it shows an empty state with Curioso.

### Settings

- Settings replaces the shell, as before, with a list of five sections on the left: General, Projects, Appearance,
  Accounts and limits and Privacy, and "Faber <version>" once at the bottom of the list. Ctrl comma opens it on General,
  the quota of a vendor in the sidebar opens it on Accounts and limits, and Esc or Back closes it. Switching sections is
  instant: no exit copy of the old section is kept, nothing on the stage animates, and the new section opens scrolled to
  its top. All sections share one content width, 720 px (`--st-w`).
- One grammar for every section (`ui/controls.tsx`). A group is a card (`--s1` with an inset `--line`, `--r-md`) under a
  small title; inside it every option is one row: the name and one short line (one line, 70 characters at most) at the left
  and the control at the right, rows divided by hairlines. The longer text of an option unfolds under its row from a small
  round "?" next to the name (`aria-expanded`, `aria-controls`, one accessible name, `set.learnMore`); there is no
  "Learn more" link. A fold (`Fold`) is a row whose title is a button; what is inside it is not in the page while it is
  closed. Switches are native checkboxes with `role="switch"`; segmented controls and selects sit in the control slot; the
  mode menu is a select-only combobox whose open list shows each choice with its own line. Text fields share one class,
  `.field`. Under 960 px the control slot moves under the name.
- General has, in groups: *You* (language, name); *How the AI works* (Permissions as a menu of the five modes, Web search,
  CLI hooks, Chat working folder with the recent folders where its control exists, and a closed "Codex" fold with
  reasoning summary and answer length); *Notifications and system* (Notify me, Play a sound, and on Windows keep the
  computer awake while Faber works and start Faber at sign-in); *Updates* (one switch, "Receive previews", off by default,
  with one short line; it acts at the next update check and turning it off never installs an older version). "Don't ask" is the default for new and
  existing settings; "Ask before commands" preserves the previous behavior. Choices save at once and stay at the saved
  value if the save fails. R107 applies the same default to Chat. Chat uses its chosen canonical folder
  (fixed before the first message), or private per-conversation scratch; deletion/draft cleanup never removes a chosen
  folder. General explains the rule and offers the folder choice.
- R140 (Chat; the person's own project sessions take the same modes, and mission roles never do): the Permissions choice is a
  menu of five modes, each with one plain line and its long text behind the closed "?" of the row, which is not in the page
  while closed. The composer of Chat carries the same choice as a chip. Mode by CLI (the CLI that lacks a mode runs the
  nearest safer one and the reply says so, `mode.fallback.auto`):

  | Mode | Claude Code `--permission-mode` | Codex approvalPolicy / sandbox / approvalsReviewer |
  |---|---|---|
  | Ask | `default` | `untrusted` / `workspace-write` / `user` |
  | Accept edits | `acceptEdits` | `on-request` / `workspace-write` / `user` |
  | Plan | `plan` | `on-request` / `read-only` / `user` |
  | Auto | `auto` (else `acceptEdits`) | `on-request` / `workspace-write` / `auto_review` |
  | Full access (default, stored `dontAsk`) | `bypassPermissions` | `never` / `danger-full-access` |

  Rules that hold in every mode: Plan never takes a remembered "Always allow" and remembers none. Moving to a stricter mode
  stops a running turn and revokes every remembered grant. The revocation is a new random token, `grantsEpoch`, written in the
  same settings save that changes the mode; each grant carries the token that was current when it was made (`allowAlwaysEpochs`),
  and it stands only while the settings hold that same token, on every check and after every restart. No clock is involved. A
  grant with no token (an older install) stands only while the settings have never held one. The conversation files are also
  cleaned, but only as tidiness: a write that failed cannot bring a grant back. Settings that cannot be read start a new token
  that is kept in memory and is the one in force until a save writes it, so a read that works again never brings the old token
  back. A card remembers the token it was made under: an "Always allow" on it is kept only if the token is still the same when the
  answer settles, and otherwise lets that one request through and remembers nothing. Two Faber processes on one data folder are
  not supported (single-instance protection prevents it; `FABER_DATA_DIR` is a dev and test hook), so no cross-process
  serialization of this is built. A stricter web setting stops a running turn but
  does not change command grants.
  No mode promises zero execution: Ask lets plain reads through, Plan lets read-only commands through, and Accept edits lets
  Codex run commands inside the working folder. Leaving Claude's plan mode names `default` for the session. Settings that cannot
  be read start in Ask, not Full access (a missing file still means Full access). Every isolated or internal Codex thread has
  `web_search` disabled. Credential-folder denies hold for Claude's file tools in every mode. For Codex Chat they are not
  applied yet: permission profiles that extend `:workspace` or `:read-only` could carry them on compatible backends, and that is
  a follow-up. Until then Faber only refuses a forwarded request that names a credential folder.

  Web search (Off, Cached, Live; Live by default in the saved Chat settings, Off for any launch nobody chose for): Codex `web_search` in the thread config; Claude Code allows
  WebSearch and WebFetch (Live), only WebSearch and denies WebFetch (Cached, which it lacks), or denies both (Off).
  Cached is offered only while Codex is connected. Codex reasoning summary and verbosity go into the thread config as
  `model_reasoning_summary` and `model_verbosity` and stay unset (the model's own) until chosen.
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
  end together (a Windows job object made before the CLI runs, failing closed; R124b, R124c). Known residual: the CLI is
  created suspended and joins its job a moment later (about 100 ms, measured on Windows 11), so if Faber ends in that
  moment the CLI stays suspended, outside any job, until the PC restarts. It never runs a single instruction. Std Rust
  cannot create a process inside a job yet (`raw_attribute` is unstable), so Faber starts as few CLIs as it can: Codex is
  asked for its models only while Faber has none, one asking at a time. Settings shows the
  details; the thread shows nothing about it (R168). The isolated mode below is `Permissions::Isolated`: Faber's
  own model list, router, internal checks and self-development roles use it, and the person cannot pick it.
- No prompt can block a full-access turn. Where the installed Claude Code declares `--permission-prompts` as an option line in a successful `claude --help` (read once
  at start, and again when a launch fails with "unknown option", which also drops the newer flags at once; present on 2.1.296), full access starts it with `--permission-mode bypassPermissions --permission-prompts none` and no host prompt
  tool: a request that would have prompted is denied at once, listed in the turn's `permission_denials`, and kept on the reply as a
  parked request, as safe metadata only (tool, a kind, a file path or a page host, the time; never a command or any input, which
  could hold a token), at most 20 and 4 KB per turn with the rest counted ("+N more"). The row of the running list says "Needs you"
  with a card that has no buttons (the CLI is past it, and the work was not done) until the next message. The tray notifies once
  per turn ("N requests need you"), with a one-minute cooldown across turns (R92). It is never allowed on its own, and a request on a
  credential folder is not parked at all: the deny rules refuse it for good. A CLI without the flag, Ask and the isolated mode start
  as before. Where the CLI lists `--autocompact`, every launch but a side question compacts at its model's window (Haiku 150k,
  Sonnet 300k, Opus 400k, R150); side questions, unknown models and a CLI without the flag keep the CLI default.
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
- Appearance is a draft, in the same cards and rows as the other sections. The first card, Look, has the six looks as small
  live renders (one look card, shared with the first run) and the accent with a hex field; the second, Text and layout, has
  the background, the two fonts, text size, density, corners and motion. A closed "Advanced" fold holds the measured contrast
  of text, faint text and the accent, the three effect switches, the fixed-meaning colours and the theme file. The controls
  change the draft, and the preview panel on the right wears the answer of `preview_look`. The panel
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
- Accounts and limits has one card per AI (the former Connected accounts and Usage limits, one section): its mark, name,
  the plan with its first letter capital, and one status word in a pill that is amber only when a window is really near
  its limit (C002; fast use is said calmly). Then one row per quota window the vendor has reported (name, used share, a
  bar with the expected-pace mark, and the reset time written in full); an old reading, or none, shows "—". The card ends
  with Faber's own spend today at API prices, written the language's way with two decimals. Every note about how the
  figures are read (counting delay, shared account, incomplete meters, learning, foreign use, extra usage, each window's
  projection and age) is behind one "Details" fold per card. "Manage accounts" appears once in the section.
- Privacy has three statements: telemetry is off, history stays on this PC, and Faber never reads the CLI logins.

### First run and tutorial

- The first run has three steps in a window with no sidebar: connected accounts, appearance, first conversation. A stepper
  at the top shows the place. Each step is a modal dialog: its heading takes focus when it opens, and Esc and the
  shortcuts of the shell (Ctrl N, Ctrl comma, Ctrl period) do nothing until a button is used.
- Step 1 is the account cards, with Continue and Skip; both go on to step 2 (or straight to the Chat home when a look is
  already saved). Opened later, from the no-AI state or from
  Settings, the same cards have no stepper and only a Close button. They save `setup` and nothing else, so reconnecting an
  account never restarts the first run or the tutorial.
- Step 2 shows the three themes side by side, each a small live render drawn with its own tokens. A choice changes the whole
  screen at once and saves nothing. Under the cards: text size (three "Aa" sizes), density, motion, "Do you already use
  AI?" (Never, A lot) and an optional name. Continue saves the appearance, the answer, the name and, unless the tutorial
  was already done, `tutorialDone: false`, in one call. Back keeps the
  choices. An install saved by an older version, whose appearance has no `base`, sees step 2 once.
- The tutorial starts at the new conversation, with an AI connected, while `tutorialDone` is false. A beginner gets three
  cards (Chat, Projects, Follow along); someone who answered "A lot" gets two (Auto, the running list); no answer counts as
  a beginner. A card has Curioso, "n of total", a title, one or two sentences, Next (Got it on the last) and, except on
  the last card, Skip.
  A ring and a pointer mark the real element the card is about (`data-tour`). When that element is missing or hidden, such
  as the Projects button below 900 px, the card is centred. The cards for the running button are pinned: that button is in
  the corner and is what they are about, so when the greeting is in the way they still sit under it with their pointer, at
  every width from 720 px, instead of floating in the middle (`placeStep` in `ui/tour.ts`).
- The tutorial is modal: the shell is inert, focus starts on Next and stays on the card, and Esc skips. Finishing or
  skipping saves `tutorialDone: true` and gives focus back to where it was, or to the composer. It never reopens. If the
  save fails it still ends for that window and may show once more at the next launch.

## Projects: self-development missions

The minimal Projects slice of R96 ([spec](superpowers/specs/2026-10-12-self-development-mvp-design.md)). The engine types
live in `engine/src/project.rs`, `project/store.rs` and `project/wire.rs`; this section is their published contract, and
screen work may start against it with mock data before the engine runs missions.

### Screens

A mission has no screen: a project is worked in by talking to its agent (R160), and the start card, the checks sheet, the
mission board and the local build card left the bundle with their 30 s poll (R171). The engine keeps the mission
machinery and the shell its commands (product-map.md, Shell commands). The Projects area shows one of two things:

- With no project open, the small "Open a project" screen (`ui/Projects.tsx`): the empty state with its one button, the
  recent folders and saved environments, and at most three project conversations to pick up.
- With a project open, the new-conversation screen of Chat (`welcome-grid project`) scoped to the folder, in one column:
  Curioso, the project and its folder in short form as one quiet line in the interface size (the last two folder names,
  the full path on hover; the sidebar and the crumb already name it), the greeting "What should we tackle?", the shared
  composer, and under it at most three of this project's own conversations to pick up, each saying only when. No
  suggestions (the composer's hints show what to ask) and no second column. No beam of light: a project screen is a place
  to work; in Dark Room the black hole keeps to the room on the right, so no words sit on its bright part. The sidebar
  lists the projects, and every setting of a project (the tool policy, the code access, the self-update switch and the
  checks) is in Settings, Projects (C011), one closed entry per project with one row per option with one short line and
  the rest behind the "?" of the row.

### Records

A mission keeps plain records in the repository's `.faber/missions/<id>/` folder and private state in Faber's data
folder. Every record has a `schema` version. A reader keeps the fields it does not know and writes them back unchanged,
and refuses a record whose schema is newer than its own (`error.journal.newer`). Ids are 1 to 64 letters, digits, `-` or
`_`.

| File | Holds |
| :--- | :--- |
| `mission.json` | goal, project, repository (its common git directory), base revision, `faber/<id>` branch, grant revision, run mode, state |
| `plan.json` | the task graph and its revision |
| `contracts/<id>.json` | owner, version, digest, base, boundary, examples, proof ids, dependencies |
| `briefs/<task>.json` | what a fresh worker session starts from: goal, non-goals, scope, base, contract, sources, proofs, grant, stop |
| `decisions/<id>.json` | question, kind, who asked, default, the answer and who gave it |
| `artifacts/<id>.json` | a hand-back: changed paths, revisions, command receipts, checks not run, cost completeness, summary |
| `receipts/<intent>.json` | an intent written before an effect, and its receipt after; an intent with no receipt is reconciled, never replayed |
| `missions/<id>/runtime.json` (data folder) | CLI sessions per task and the leases with their processes; never in the repository |

In `projects.json` a project gains `repo` (its common git directory) and `grant` (mode, revision, author, remote,
self-update trust, revocation). A project saved before missions has neither and allows no effect. Seguro allows
worktrees, commits with a known author and fast-forwards of the mission's branch. Equilibrado adds the push of that
branch and one draft pull request to a named remote. Self-update needs its own trust. Each check names the grant
revision the work was admitted under, so a new revision or a revocation stops the next effect (`reason.grant.*`).

Writes replace the file atomically and flush it; a failure leaves the old file and returns `error.store.full` or
`error.store.write`. A damaged mission record, runtime sidecar, project list (read for Projects) or quota ledger is
`error.store.corrupt`, never empty data. A damaged `budget.json` is copied to `budget.json.quarantine`, and Projects stays
closed while that copy exists, even after Chat has saved a fresh ledger over the damaged one.

The engine refuses a plan with a cycle, a missing or repeated task, a deliverable claimed twice, a write path outside
the repository, writes on a read-only task, or two writers that nothing orders sharing an area, parent folders and case
aliases included (`error.plan.*`). A contract's digest is SHA-256 over its content with sorted keys. A revision sends the
tasks built on the old digest, and everything that depends on them, back to Blocked with `reason.contract.revised`;
nothing else moves.

### Worker envelope

Every worker turn ends with one JSON object, version 2, with no unknown fields. Durable journal records remain version 1;
old records stay readable and retain their extension data. Wire version 1 is not accepted for a new dispatch.
The engine refuses an envelope over
512 KB, a stale task, attempt, base, contract digest or lease generation (`error.wire.stale`), an action outside the role
(`error.wire.role`), and a path that leaves the worktree (`error.wire.path`). A file write carries the SHA-256 of the
content it replaces, or none for a new file.

```json
{"schema": 2, "task": "fix", "attempt": 1, "base": "3d3d502", "contract": "<sha-256>", "lease": 4,
 "action": {"patch": {"ops": [
   {"write": {"path": "engine/src/x.rs", "expect": "<sha-256 of the file as read>", "content": "..."}},
   {"rename": {"from": "a.rs", "to": "b.rs", "expect": "<sha-256>"}}
 ]}}}
```

| Action | Who | Carries |
| :--- | :--- | :--- |
| `read` | every role | up to 64 paths, and symbols whose callers to find |
| `plan` | lead | summary, acceptance, tasks (id, title, kind, requires, decisions, contracts, deliverables, writes, proofs) |
| `amend` | lead | tasks to add or replace, ids to remove, reason |
| `patch` | writer; test proposal, kept as an artifact | up to 64 write, rename and delete operations, each with its expected hash |
| `proof` | lead, writer | named proof ids, never command text |
| `decision` | lead | question, kind (technical, direction, taste, money or legal), options, default |
| `complete` | every role | summary, changed paths, assumptions, risks, checks not run, a reviewer's verdict and findings, next |

The AI answers only technical decisions; direction, taste, money and legal ones wait for the person
(`error.decision.person`), in autonomous mode too.

Wire v2 adds optional fields without changing role authority. `ping` is `Order`, `Found`, `Blocked`, `NeedEyes`,
`DoneReady` or `Disagree`; `note` is at most 60 whitespace-delimited words. Envelope `requires`, `checks` and `quotes`
are verbatim-text ids; task `requires` continues to mean task dependencies. Briefs retain each id's exact UTF-8 text.
Task drafts add `labels` (verb, at most eight normalized needs, risk, freshness and facets), scoped `paths`, check ids
and a `kit` receipt/digest pin. Plans add the deliverable `sheet`, grounded `bar` and `compileReceipt` reference.
Completions add a check `receipt` id, bounded check outcomes, artifact paths/SHA-256 hashes, claims tied to checks,
assumptions with provenance, `unconfirmed`, `kitUsed` and newly requested needs. Unknown nested fields remain errors.

`wire::CheckReceipt` is distinct from the journal's effect `Receipt`; it binds checks and evidence to one candidate,
model, effort and known or unknown cost, with links to effect receipts. Check results are pass, fail or not-run:
pass requires evidence; failure and not-run require a reason. `CompileReceipt` records phase timestamps and cost tags.
These types publish the shared T5K/T5B/T5 contract. A worker's claimed receipt is data; only engine-verified proofs and
the independent exact-tree review can authorize acceptance. Missing optional fields do not imply a passing bar or kit.

### Automatic project kits

`project/kit.rs` and its submodules implement the pure T5K contracts. Cards are written from reported inventory,
scoped project sections and named engine proofs; kits are assembled from needs, never saved as hand-written loadouts.
`Inventory::new` validates, hashes and indexes an immutable snapshot once per inventory change. Assembly uses that
snapshot, origin priors, job ownership, conflicts, role budgets and safety filters. Versioned Beta outcomes only break
ties. A blind, shuffled shortlist supports shadow comparisons; the code selection remains authoritative.

Persist the selection with `Journal::record_kit` before dispatch. Compose approved, hash-checked inline bodies into
the existing safe role prompt with `compose_role_prompt`; the role prompt remains its byte-for-byte prefix. This
adapter changes no driver flags or policy. Record init with `attest_kit` before work, then add the outcome with
`complete_kit`. Completion cannot replace the selection or its attestation. Receipts live in
`.faber/missions/<id>/kits/<task>-<attempt>.json` and preserve additive root and nested metadata. Card caches and cited
field briefs do the same; a newer durable schema is refused as newer. Dispatch and card-writer replies retain their
strict schemas. A failed write prevents dispatch.
The runner must enforce T2 admission and handle mismatched/unreported init; these receipts never grant containment.

G0 is empty. G1 returns at most three cited suggestions with downsides, with installation parked. G2 stores bounded,
cited primary-source briefs in `.faber/briefs/`; G3 retains verified role controls. Only a missing required capability
parks its dependent node. The pure start-card adapter supplies two locale keys; T9 owns their screen and translations.
The Auto labels adapter shares the existing classification turn and preserves its model-choice table. T5 integrates
classification, fresh pass sessions, dispatch, proofs and two-order comparisons; T5B resolves native bar proof IDs.

See [the T5K proof report](research/2026-10-12-project-kits.md) for measured synthetic performance and the live G9
evidence still required by T13. Fixture attestation is not proof of native isolation.

### Proof isolation (R138)

A mission proof, and the canary's build and test of the code its Writer changed, run code a role wrote before any review.
The engine starts them through `proc::isolated_spawner`: the recorded environment only, created suspended, joined to a
kill-on-close job, and on Windows lowered to Low integrity before the first instruction. The mission clone, the proof's
build folder, its own cargo home and npm cache, and its temporary folder are labelled Low (inheritable), so the proof can
write there and nowhere else: Windows refuses its writes to the installed app, the engine's records, the owner checkout
and the user profile (except folders that are Low by design, such as `AppData\LocalLow`). Where the user may only Modify
a folder (a second drive), the folder's owner first grants itself the right to label it. On macOS `sandbox-exec` denies
writes outside the same folders.

Residual in v0, disclosed on screen (`projects.proofs.isolationTech`): a proof can still read what the user can read and
use the network, and on Linux nothing limits its writes. R139 accepts this for v0: chat roles already run with full access
(R121), which exposes strictly more, and a proof starts from a cleared environment at Low integrity. The upgrade path is an
AppContainer (Windows), which closes reads and the network as well.

The real canary is opt-in (`real-canary.json`). It runs the candidate build, untrusted until it passes, with the real
Claude Code and Codex CLIs, signed in through the user's own profile and environment, with copied qualification receipts;
only its proof runs isolated.

Two more launch checks: every qualification receipt binds the launch-policy revision (a SHA-256 of every role's launch
parameters), so a changed flag or setting asks for a new qualification run; and a role CLI is created suspended, the file
its image came from is pinned (opened with a share mode that denies write and delete) and hashed through that handle before
`--version` runs and again before resume; the handle stays open until the process has started, so the file cannot be
swapped in between, and a hash other than the receipt's kills it before it runs. Waiver markers and their ledger records
live next to the canary data folder, not inside it, so deleting that folder does not revive a spent waiver, and a waiver
is recorded as spent before the mission that uses it runs.

Follow-up: a crash after a proof ran but before its progress mark was written replays the proof on resume. Proofs are
idempotent tests, so v0 accepts the replay.

### Screen commands and event

The commands fail with a key string, like the other shell commands, and never report a success they did not have.

| Command | Request | Returns |
| :--- | :--- | :--- |
| `project_enroll` | `{projectId}` | Settings, Projects: repository, base, author, selected mode, revision, self-update, remote |
| `project_grant` | `{projectId, mode, selfUpdate, revision}` | the grant |
| `project_revoke` | `{projectId}` | nothing |
| `mission_start` | `{projectId, goal}` | the mission id |
| `mission_state` | `{missionId}` | the mission view |
| `mission_mode` | `{missionId, mode}`, mode `normal` or `autonomous` | the mission view |
| `mission_stop`, `mission_resume` | `{missionId}` | the mission view |
| `mission_answer` | `{missionId, decisionId, answer}` | the mission view |

The `mission` event carries the same view whenever it changes. Its `revision` grows with every view and the screen
ignores an older one. Cost is in millionths of an API-equivalent dollar; `null` is unknown, never zero.

Implementation status on `selfdev-t9`: the screen and fixed command adapter can display durable mission snapshots,
but the approved T5 runner is not in this branch. Start and Resume return `reason.mission.runnerUnavailable`; an old
`running` journal is displayed as blocked. Direct project send, warm, draft routing and branching are refused at the
shell boundary, including quota/paid-credit recovery, while saved conversations remain readable. Equilibrado
publication stays deferred (R103).

The board restores the selected mission, rejects other mission identities and older revisions, and keeps task evidence
behind a keyboard-reachable disclosure. `TaskView.detail` is an optional additive field containing scope, required
proof IDs and durable artifacts; absence never means verified. Build slots without a T11 snapshot remain visibly
unknown. The cap, measured spend, daily room and reading time come from the existing admission ledger, with unknown
values preserved. Stages narrower
than 760 px use one scrolling list. Mission notification receipts use the existing atomic store and R92 policy;
hidden test builds inspect that policy without submitting OS notifications or playing audio.

New non-English keys are English draft fallbacks marked `Native speaker must translate (needs native review)`.
They require native review and are not accepted translations. These implementation limits do not release T9/T10's
live-runner integration gate.

```json
{"missionId": "m1", "revision": 7, "state": "running", "mode": "normal", "gitMode": "seguro",
 "goal": "Fix the failing normalization test", "plan": "One writer fixes clean_path; the other vendor reviews.",
 "tasks": [{"id": "fix", "title": "Fix clean_path", "state": "running", "reason": null, "builder": "codex", "reviewer": "claude"}],
 "cost": {"ownMicros": null, "capMicros": 2000000, "complete": false},
 "pace": [{"vendor": "claude", "window": "seven_day", "used": 0.31, "room": 0.04, "readAt": 1791000000.0}],
 "decisions": [], "build": null, "reason": null}
```

The keys these return (`error.store.*`, `error.journal.*`, `error.plan.*`, `error.wire.*`, `error.decision.*`,
`reason.grant.*`, `reason.contract.revised`) get their hand-written texts in every locale together with the Projects
screen, which owns the locale files.

## First frame and update progress (R157)

- `ui/index.html` paints a static first frame before any script runs: the frame colour of Dark Room, or of the light
  theme when the system asks for light, with the mark centred at half strength. No text (so no language is needed) and
  no animation, reduced motion or not. Its rules sit at the top of `ui/styles.css`, never in an inline `<style>`: the
  shell adds a hash to the page's CSP for an inline style, and a CSP with a hash ignores `'unsafe-inline'`, which
  blocks every style the page adds later (the screen sentinels' planted defects among them). `main.tsx` removes it once the screen mounts, and the saved look takes over.
- The window is built hidden. `screen_ready` shows it after the screen's first frame; a 3 s failsafe shows it anyway.
  A second launch before that moment waits for the same reveal. Because the page paints its own frame, a reveal that
  comes early or by the failsafe still shows the mark on the theme colour, never a blank window.
- While Faber looks for, downloads or installs an update, the opening screen shows the progress meter: a percent, or the
  megabytes when the size is unknown, then "Installing" (`ui/UpdateStage.tsx`).
- `tests/ui/first-frame.check.ts` asserts the page carries the mark, `styles.css` its background for both schemes, and the page no inline style.

## Rules for every screen

- Every text on screen lives in `ui/locales/`, written by hand in each language.
- Choice buttons use `aria-pressed`, or native radio groups. Switches are native checkboxes. "Open file" is a real
  button. Theme thumbnails are `aria-hidden`, and each theme card is named by its name and its description.
- Areas whose content can grow scroll; they never cut content off.
- Right-click is Faber's own (R123, `ui/contextmenu.ts`, `ui/ContextMenu.tsx`). The browser's menu never shows, and its
  reload, print, save and view-source shortcuts do nothing. A menu opens only on a chat (Rename, Delete), a project (New
  conversation in this project, Open folder, Remove from Faber), a message (Copy; Edit on yours; Redo on an answer),
  selected text (Copy) and a text field (Cut, Copy, Paste, Select all, with the ones that cannot apply disabled); anywhere
  else nothing opens. Each menu has at most four items, only actions that already exist on screen, and a destructive one
  goes last in the danger colour and asks twice by changing its own text; the second activation must be a separate gesture (one that follows within half a second is ignored). The engine refuses to remove a project while a conversation or a mission works in it, and "Open folder" only reveals a folder that exists on this computer (never a program, address or network share). The menu opens at the pointer, or by the Menu key
  and Shift+F10 on a focused row; arrows move, Enter chooses, Esc and a click outside close it, and it stays inside the
  window. It uses the normal entrance and exit and is instant in reduced and no motion.
- Every overlay (the first-run steps, the tutorial cards, the narrow-window sidebar, the overlay running column) says
  whether it is modal. A modal overlay moves focus into itself when it opens and gives focus back when it closes. Esc
  closes it, except where a screen says otherwise. The one non-modal case is the running column when it opens by
  itself: it moves no focus at any width, nothing behind it becomes inert, and it ends above the composer so the
  composer is never covered. Opened by the user, the overlay is modal.

## Screen cards

One card per main screen: what it is for, the one action that leads, what never goes on it, and the photo a critique
compares against (R172). What the screen holds is in [product-map.md](product-map.md#screens-and-surfaces) and how it
looks in the sections above; a card only says what must not drift. The approved photo is the one the UX sentinel's
composition scenario takes (`target/faber-test/shots/`, kept by CI with every run), at 1920x1080 and at 1280x800 in Dark
Room. A change that moves a card's line updates the card in the same commit and says so in its visual critique. Today's
photo of a project's home breaks its card (the findings in `tests/ux/baseline.json`); the first one after the lane
project-home-calm lands, with those lines gone, is its approved photo.

| Screen | For | Main action | Never on it | Approved photo |
| :--- | :--- | :--- | :--- | :--- |
| Chat home ([map](product-map.md#screens-and-surfaces), [layout](#the-new-conversation-screen)) | Starting or picking up a conversation | Writing in the composer | Settings, switches, a second model picker, another area's rows | `critique-chat-home-1920x1080.png`, `-1280x800.png` |
| A project's home ([map](product-map.md#screens-and-surfaces), [layout](#the-projects-home)) | Giving the agent work in this project's folder | Writing the goal in the composer | Chat's suggestions, hints or conversations; the project's settings (Settings, Projects); art behind the words; the project's name more than twice | `critique-project-home-1920x1080.png`, `-1280x800.png` |
| An open conversation ([map](product-map.md#screens-and-surfaces), [work log](#work-log)) | Reading the work and answering it | Answering in the composer, or the card that needs the person | Settings beyond one status line; technical logs outside a fold; motion on text being read | `critique-conversation-1920x1080.png`, `-1280x800.png` |
| Settings ([map](product-map.md#settings-screen), [layout](#settings)) | Changing how Faber behaves, one short line per option | Choosing an option in the section that is open | Work (a conversation, a goal); explanations longer than a line outside "Learn more"; a new section where one fits | `critique-settings-1920x1080.png`, `-1280x800.png` |

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
only the jobs a change needs (R163). A first job reads the changed files and picks the suites by the same rules as
`cargo t` (`testkit/suites.json`, read by `testkit/src/plan.rs` and `scripts/ci-scope.mjs`): words only (`docs/` or a
`.md` file) need none, `docs/product-map.md` needs the `rust` suite because a test reads it, the test files of the engine
and the shell need only the `rust` suite, a manifest, the runner, CI or a file no rule knows needs every suite, and any
file that is not words runs the lints. A push is compared with the commit before it, and a queue commit with
`phase-1-base`, where it lands; either may skip a suite only when the commit it builds on is green, and anything the job
cannot tell runs everything (R165). Every suite still runs once a day on `main`, and on a commit before it becomes
stable (`promote-stable.yml` starts the run and waits for it). The lints and the `rust` suite run on Windows and on macOS
(macOS adds the `ui` suite to its lints). One Windows job builds the test app once (`cargo t --build-only`, the fake
CLI and the cache example at the same time in Cargo's own folder) and hands its programs (`built_programs` in
`testkit/src/testbuild.rs`) to a `ui` part and eight `screen` parts, which build nothing
(`--suite screen --shard 2/8 --prebuilt`) and use the Rust the runner image has. On CI the runner uses every core; on a
person's PC it keeps to half. Every Cargo the runner starts goes without the variables `cargo run` gave it, which made
ring's build script, and everything above it, build again. Each kind of job keeps its own Rust cache, and only `main`
saves it: a run can read only the caches of its own branch and of `main`, and branches that each saved their own pushed
the others out of the 10 GB GitHub keeps. Since pushes made by CI start no run, the `land` and `main` jobs start a warm run
on `main` after it moves, which builds what each kind of job builds and saves it when the dependencies changed. The runner
deals the scenarios so the parts take about the same time: the slowest first, each to the lightest part so far, by the CI
seconds of each scenario. A `costs` job reads them from the logs of the newest green runs (`scripts/screen-costs.mjs`) and
hands every part the same table (`--costs`), so the dealing never ages; without those logs, and on a person's PC, the copy
in `testkit/screen-costs.json` counts, and a scenario missing from it counts as 5 s. CI keeps the Windows screenshots as artifacts.
Anything outside the page (tray, notifications, installer, updates) is checked through the operating system's accessibility layer and file versions.
The update scenario (T13 4) runs the update flow's clock five times faster from one anchor all its processes share
(`FABER_TEST_CLOCK`, read only by the test build; a release build with the test hooks does not compile), so it proves
the whole 300-second probation and its 30-second health window in about 100 s instead of 335 (R163).

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

Many lanes run `cargo t` on one PC; each run peaks near 8 GB. On 2026-10-16 and 17 lanes spent 441 minutes waiting for a
slot, so the suites now run in CI on the queued commit and a lane runs `cargo t --claims` before a commit, which builds
only the runner, takes no slot and checks the claims and the red base (R-next). Before building, a run takes one of three slots
(`testkit/src/slots.rs`): a locked file `target/.faber-slots/slot-<n>` of the main checkout, held until the run ends,
crash included. With all three taken it prints one waiting line and waits. `FABER_BUILD_SLOTS` sets another count for
another PC (R129, E3).

### What a run prints

A run prints only what needs a look (R154): each failure with its reason, each pass that measures memory or processor
(R125), and one summary line. A failed Rust test shows its panic from the `file:line` on; a compiler error shows as
`file:line:col: error[code]: message`, with the compiler's suggested fix as one `help:` line under it. A reason longer
than 15 lines keeps its first line and its error lines. Every line, every pass and Cargo's whole output included, goes
to `target/faber-test/run.log`, which CI keeps with the screenshots. `cargo test` runs with `--no-fail-fast`, so one run
lists every failing test. `cargo t --suite lint` runs the two Clippy proofs the same way, and a test keeps them equal to
CI's lines. The `cargo t` alias builds the runner itself with `--message-format=short`, so a broken build of the runner
also prints one line per error.

On Windows the lint suite adds a third proof: Clippy for macOS (`aarch64-apple-darwin`) on the engine and the runner, so
code behind `cfg(unix)` or `cfg(target_os = "macos")` breaks before the push, not a CI round later (R166,
`testkit/src/cross.rs`). Clippy links nothing, but `aws-lc-sys` compiles C, so zig stands in for the Mac's C compiler:
Cargo calls the runner as `CC` and `AR` and the runner hands the call to zig. It needs zig on `PATH` and
`rustup target add aarch64-apple-darwin`; without zig the proof is skipped and says so. The shell is left out, since its
Objective-C needs Apple's SDK; CI's Mac still checks everything.

A mission reads its checks the same way (R155, `engine/src/project/proof/output.rs`, which `cargo t` also uses for
Cargo). The roles get each failure with its `file:line` and the tool's fix, the measurements and one summary line, at
most 20 failures of 8 lines; the whole output stays in the mission's private `proofs/` folder. It reads `cargo t`,
rustc and Clippy, `cargo test`, `cargo fmt --check`, tsc and oxlint; any other format gives its last 20 lines when the
check fails. A new format is one more parser in that file's list. Lines that look like a secret are withheld first.

### Lane claims

Lanes that run at once (R128) say first what they will change (R154). A worktree on a branch writes `.faber-lane` at its
root before it touches a file; git ignores the file:

```text
area: Settings screen
own: ui/Settings.tsx
own: engine/src/settings/
share: ui/locales/
```

`area` is a row or heading of [product-map.md](product-map.md), or `none` for work no person sees. `own` is the lane's
alone; `share` is a file several lanes may append to, such as the locales or the decisions. A path claims itself and
everything under it. Before any suite, `cargo t` (`testkit/src/lanes.rs`) stops when a changed file is not claimed, when
an area is not in the map, or when a claim overlaps another worktree's (two `share` claims may). The main checkout (the
lead session, CI) and detached worktrees (Faber's own missions, test workspaces) need no claim. A lane deletes its claim
once its last commit is on `phase-1-base`; a claim nobody saved for a day is stale and ignored. Claims live on one PC:
a lane on another machine is not seen.

### Merge on green

`phase-1-base` requires one check, `gate` (R173): a job in `ci.yml` that is green only when `changes`, `check`, `app` and
`screen` passed or were left out on purpose by the plan. The rule is not strict, so a commit behind `phase-1-base` still
lands; the run on `phase-1-base` that follows is what `main` waits for (R127). `main` and the queue (R162) run after the
gate, so the queue only pushes a commit whose gate passed.

Nobody waits on CI (R-next). Right after opening a pull request, its lane runs `node scripts/land.mjs <number>`
([Numbers](#numbers)) and goes on to its next task. `land.mjs` puts a pending `queue` status on the pull request's head.
When the queued commit's gate or its landing fails, the `report` job turns that status red and comments on the pull
request with the run, which wakes the session that watches it (the app's auto-fix). The fix is pushed to the pull
request and queued again with `land.mjs`; a push alone never lands, since the landing stops at a head it did not squash.
`gh pr merge --auto --squash` still works for a pull request without numbers or conflicts that `land.mjs` cannot take,
such as one from a fork: GitHub squashes it the moment the gate is green.

### Task queue

A session that finished a task takes the next one by itself, without asking, the way it does when the owner says "I am
going to sleep, keep going" (R-next). The list is `docs/tasks/`: one file per task, `<order>-<name>.md`, lowest order
first, with a `# Title` line and the brief a fresh session needs. One file each means adding, reordering or finishing a
task never touches another's lines. The owner decides what goes there; a session may add a follow-up it found after
the owner's tasks.

`node scripts/tasks.mjs next` reads the list on `origin/phase-1-base` and claims the first free task by creating the
branch `claim/<name>` on GitHub. A push without force fails when that branch is already there, so two sessions never
get the same task, whatever PC they run on. The claim is a commit of its own whose message says who holds it and since
when; one older than a day is stale, and the next session takes it over (the same day as lane claims). The session
works on a branch named after the task, and its pull request deletes the task's file, so the task is gone once it lands;
the next `next` deletes the claims of tasks that left the list. `list` shows who holds what, `release <name>` gives a
task back. CI never runs on `claim/` branches.

A session asks the owner only what is the owner's to decide (direction, taste, money), in one short card, and takes the
next task while the question waits. It stops when `next` says no task is free.

### Red base

A commit pushed onto a red `phase-1-base` waits behind the failure, and `main` cannot follow it (R127); on 2026-10-16 a
macOS-only check kept it red for three hours while ten commits landed on top. So the red base shows before a push
(R164). `scripts/base.mjs` asks GitHub through `gh` for the newest finished push run of `ci.yml` on `phase-1-base`
(cancelled runs say nothing) and, when it failed, reads the runner's `FAIL` lines and compiler errors from its failed log,
each with its job, or the failed jobs and steps when the log names none. It keeps the answer three minutes in the shared
git folder (`.git/faber-base.json`). Every `cargo t` outside CI prints those lines first, marked `BASE`, and writes the
`pre-push` hook (`testkit/src/base.rs`) unless the person has a hook of their own. The hook refuses a push to
`phase-1-base` while it is red, or one that carries a number placeholder; `FABER_PUSH_ON_RED=1 git push` pushes the fix,
and a push to any other branch only prints the warning. Offline, signed out, or without Node or `gh`, it says nothing and
refuses nothing: it is a warning light, not a gate.

Local proofs run on Windows, so a check that holds only there turns the base red on macOS. A check that measures text or
pixels measures the glyphs (`Range.getBoundingClientRect()`), never the box that holds them, waits for
`document.fonts.ready`, and keeps a margin wide enough for the other system's font rendering; the bound and the worst
value seen on Windows go in its comment.

### Numbers

Decision numbers (`docs/decisions.md`) and what's-new ids (`notes.<id>.<kind>`) are given when a change lands, never in
a branch (R164, R-next): lanes that each took "the next one" collided at every merge, and lanes that each appended to the
end of the decisions table and of the five locales' entries conflicted at every merge. On 2026-10-16 and 17 that loop
(merge the base in, wait for CI, find the base moved again) cost 590 session-minutes, more than any other step.

So a branch never touches those shared lines. Its entry and its decision rows go in a file of its own,
`docs/next/<branch>.json` (a branch named `a/b` writes `a-b.json`):

```json
{
  "notes": { "better": { "en": "…", "pt-BR": "…", "es": "…", "fr": "…", "de": "…" } },
  "decisions": ["| <placeholder> | 2026-10-17 | The decision. | AI |"]
}
```

`notes` holds an entry per kind it names, each with an id of its own and a text in every language of `ui/locales/`,
each written by hand. A row of `decisions` is written like the table's, with `R-next` for its number. Elsewhere the
branch writes `R-next` for its decision (`R-next-2` for a second) in every place that refers to it, the pull request
title included. Quoted in backticks the placeholders are only words, as here. Two branches then conflict only where they
change the same lines for real.

`node scripts/land.mjs <pull request>` lands it: it squashes the pull request onto the newest `phase-1-base` with
`git merge-tree` (the working tree is never touched) and names the commit after the title and number. In a throwaway
worktree, `node scripts/numbers.mjs assign --amend` folds the fragment in after the last entry of each locale and the
last row of the table, and every placeholder, the commit message's too, takes the next free number after that base. The
numbered commit goes to `queue/pr-<number>`, where the queue checks that very commit (R162), so the base and the
required `gate` (R173) only ever see checked commits. Its land job, one landing at a time, moves `phase-1-base` and the
pull request's branch to the landed commit in one atomic push, from the head that was squashed only (the commit's
`Pull-request-head` line), so GitHub shows the pull request merged; a later push, a close or another target stops the
landing, and `land.mjs` replaces a queued commit only when nothing newer was queued meanwhile. Two decision rows that
share a placeholder stop the numbering. When `phase-1-base` moved during the checks, the land job runs
`land.mjs --head` again on the new base instead of a rebase, whose numbers would collide, and the queue checks it again.
`--dry-run` shows the commit it would queue and pushes nothing. A fragment that cannot fold stops `land.mjs` with its
reasons, and `scripts/numbers.test.mjs` checks every waiting fragment wherever the lints run.
It also fails on a decision number used twice, and on any placeholder or fragment left when CI runs on `phase-1-base` or
`main`; the `pre-push` hook refuses the same before it leaves the PC. The older form, `notes.next.<kind>` written
straight into the locales and `node scripts/numbers.mjs assign` run on the branch, still works.

A pull request with numbers never sets `gh pr merge --auto` (R173): GitHub would squash the placeholders onto
`phase-1-base` unnumbered, and the base's check would fail.

The shared docs merge themselves too (R-next). Two lanes that each add a row, a line or a paragraph at the same place of
`docs/product-map.md`, `docs/design.md` or any other `.md` file conflict in plain git, though both additions belong. So
`land.mjs` squashes with `merge.conflictStyle=diff3` and settles each conflict in which neither side changed a line the
base had: the base's new lines first, then the pull request's (`insertions` in `scripts/land.mjs`). Anything else stops
the landing with its files: a conflict in code, a file one side deleted, or a line both sides changed, which is a real
disagreement for the lane to settle. The squash is checked like any other, `engine/tests/product_map.rs` included. A
conflict that GitHub shows on a pull request in those files alone needs nothing: the landing settles it.

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

### Visual critique

Checks see code and motion, not a screen as a whole (R172). The sentinel's scenario "the main screens keep their
composition" photographs the Chat home, a project's home, an open conversation and Settings at 1920x1080 and 1280x800 in
Dark Room and pt-BR (`target/faber-test/shots/critique-<screen>-<size>.png`). `node scripts/shots.mjs <label>` runs it and
copies the photos of the screens the diff touches into `target/faber-test/critique/<label>/` with an index;
`scripts/shots.test.mjs` keeps the choice of screens. A fresh session scores the `before` and `after` folders out of 40
with `.claude/skills/visual-critique/SKILL.md`; the score never drops and a touched screen reaches 32.


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
minutes for the 5-minute one, nothing when neither is named) and the latest reply shows it in the tooltip of its cost, hidden when
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

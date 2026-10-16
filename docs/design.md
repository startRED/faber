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
  characters on one canvas (ui/Hole.tsx, --amb-hole and --hole-colors): the whole shadow, centred in the room from 150 px right of the column with its arc clear of every edge (16% to 30% of the height), an amber disk sweeping in from the lower left
  with fibres that turn faster near the hole, the far side of the disk bent over the top as a cool arc, a photon ring, a
  dim glow, a small planet below it and stars. It has no hard edge beside the column of words: behind the column it keeps 14% of its light, comes up to full
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
- The footer has Settings, the person's name when they gave one, one line per connected vendor and the version. A
  vendor's line shows its shape, its name, the used share of its tightest quota window, how old that reading is and
  "still counting" while the vendor has not counted Faber's latest use. It opens Settings > Usage limits. A vendor with no
  reading yet, or only an expired one, shows "—".
- Search filters conversation titles and project names on screen; full-text search comes in Phase 2. The percentage and
  "stopped" tails come with the progress view in Phase 3.

### The new conversation screen

- A greeting, "Where do we start?", ending with the person's name when they gave one.
- Three hand-written suggestions per language. Clicking one fills the composer and moves focus there. It sends nothing.
- "Pick up where you left off" lists up to four conversations of both areas: those that need the person first, then the
  most recently updated.
- The composer keeps its model picker, "AI: choose for me".
- Auto chooses both the model and its effort. A manual model adds an adjacent low, medium or high effort choice,
  defaulting to medium (R102). Each conversation remembers its choice. Changes apply to the next message only;
  the composer says so, and each answer keeps the effort actually used. Both CLI drivers always receive that effort.
- The ambient effect of the theme runs here, and the empty composer idles with a typing bar and rotating hints.
- With no AI connected, the greeting, the suggestions and the list give way to an empty state with Curioso and a button
  that opens account management. The composer stays on screen but cannot send.

### Work log

- Each step is a quiet chip: a verb in the ink color ("Reading", "Running") and what it acts on in the mono face, cut
  in the middle. A tool Faber does not know keeps its own name; an MCP tool reads "Using a tool" with its server's name.

### Subagent steps

When the agent hands part of the work to a subagent (R161), the reply shows one row per helper between the work log and
the answer, in the work log's surface and type: a check, a cross or the live mark, the few words it was given in bold, and
on the right its kind, its state and its step count in the third ink. The row opens like every disclosure (`animateDetails`)
to the tool chips it ran and its report as Markdown; nothing else moves. Rows sit in a `minmax(0, 1fr)` grid, so a long
tool line ellipsizes instead of widening the reply. A helper that never finished reads "stopped" once the reply has
ended; one sent to the background keeps the live mark until its task notice arrives; a refused one reads "declined" in
the third ink, never as a failure. Known limit: when a reply moves to another AI after a quota stop, its open tools,
helpers included, read as working again with the new reply.

### Recent folders and environments

The "Open a project" screen (Projects with none open) and the Chat folder setting list the last five folders used, newest
first, one click each: a short name and the dimmed parent path. The engine judges each one when the list is read
(`recents` command, `engine/src/recents.rs`): a folder that is gone or refused by R124 shows dimmed with its reason and
cannot be picked, and a refused folder is never stored. Right-click on a row opens a one-line menu, "Remove from recents".
In the project area a "+" on each row builds a selection that can be opened at once or saved as a named environment (up to
12 environments, 8 folders each; the same name replaces). Starting an environment opens the folders that still exist as
projects. Faber has no worktree creation for projects, so an environment has no "own worktree" flag. Both lists live in
`recents.json` next to `settings.json`, written through a temp file and renamed.

### Running column

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

- Settings replaces the shell, as before, with a list of five sections on the left: General, Appearance, Connected
  accounts, Usage limits and Privacy. Ctrl comma opens it on General, the quota of a vendor in the sidebar opens it on
  Usage limits, and Esc or Back closes it.
- General has, in order, the language, name, CLI hooks switch, Permissions choice, Chat working folder (with the recent
  folders), Notifications (Notify me, Play a sound), Updates (one switch, "Receive previews", off by default, with one
  short line; it acts at the next update check and turning it off never installs an older version) and, on Windows,
  System (keep the computer awake while Faber works, start Faber at sign-in). "Don't ask" is the default for new and
  existing settings; "Ask before commands" preserves the previous behavior. Choices save at once and stay at the saved
  value if the save fails. R107 applies the same default to Chat. Chat uses its chosen canonical folder
  (fixed before the first message), or private per-conversation scratch; deletion/draft cleanup never removes a chosen
  folder. General explains the rule and offers the folder choice.
- R140 (Chat; the person's own project sessions take the same modes, and mission roles never do): the Permissions choice is a
  list of five modes, each with one plain line and its long text behind a closed "Learn more" that is not in the page
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
  as the Projects button below 900 px, the card is centred.
- The tutorial is modal: the shell is inert, focus starts on Next and stays on the card, and Esc skips. Finishing or
  skipping saves `tutorialDone: true` and gives focus back to where it was, or to the composer. It never reopens. If the
  save fails it still ends for that window and may show once more at the next launch.

## Projects: self-development missions

The minimal Projects slice of R96 ([spec](superpowers/specs/2026-10-12-self-development-mvp-design.md)). The engine types
live in `engine/src/project.rs`, `project/store.rs` and `project/wire.rs`; this section is their published contract, and
screen work may start against it with mock data before the engine runs missions.

### Screen inventory

The board keeps the titlebar, sidebar, topbar, running column, tokens, type and Curioso empty states, and reads like the
start card it came from: one column up to 860 px, no cards around the header. First the state (a dot and a word) with
"New mission" beside it once the mission is done, paused or blocked (a paused or blocked one asks first, since nothing
lists it later), then the goal (at most three lines, the rest on hover), the step it is on in one line, why it waits in
one line, and the one action that moves it (Stop or Resume). Then the needs-you cards, the report, and the tasks as one
list grouped by state, only the groups that hold something; before the plan, one line says there are no tasks yet. Run
mode, cost, account pace and the local build sit folded under "Run mode, cost and build". Nothing needs a horizontal drag.

| Surface | Minimum behaviour |
| :--- | :--- |
| Goal | The composer's styling: the goal, a short plan and Start. The text stays after a refused enrollment or start |
| Start card | Repository, starting revision, git mode (Seguro selected), author and self-update trust; granted once, never per step |
| Board | Ready, Running, Review, Done and Blocked groups, empty ones hidden; stable task rows with builder, reviewer and reason |
| Cost | Own cost against the live cap; the account's daily pace and its freshness beside it, separately; unknown says unknown, folded away; the step line shows the cost only once it is known |
| Decisions | The Ask and running cards, with author and default; nothing approves an effect by itself |
| Stop | A keyboard-reachable button while running; then the paused checkpoint and Resume |
| Task detail | Scope, diff and artifacts, command results, missing checks and risks; technical logs behind a click |
| Local build | Current, next and last good, and why promotion waits; no completion toast |

Status colours always come with words and shapes. Amber stays for needs-you only: a quota wait or a dependency is not
shown as a decision.

### The Projects home

The first screen of a project uses the layout of the Chat home (`welcome-grid`) with its own identity: the greeting
"What should we tackle?", one column: Curioso, the project and its folder in short form (the last two folder names, the
full path on hover), the greeting, a goal composer with its own hints and one quiet status line beside its button, and at
most three conversations to pick up. Nothing else: the sidebar lists the projects, and every setting of a project (the
tool policy, the code access, the self-update switch and the checks) is in Settings, Projects, one closed entry per
project with one short line per option and the rest behind "Learn more". The checks are asked for just in time: the
first start on checks nobody confirmed opens a compact sheet with their count and source, every command (and the setup)
listed in full with no collapse, a long list scrolling inside it, and "Confirm and start"; what is confirmed is exactly the list
shown, and a project that changed it meanwhile is refused so the sheet shows the new list. It does not come back until
the checks change. No beam of light: a project screen is a place to work. The two layouts share classes, so a change to one home reaches the other.

C011 (2026-10-16, the owner) overrides the quiet rows: the project screen shows only the work, and the grant, the checks
and the tool choice live in Settings or in a just-in-time consent. The start card is done; the mission board keeps
the run mode folded under "Run mode, cost and build" (product-map.md, Known incoherences, 1).

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
| `project_enroll` | `{projectId}` | the start card: repository, base, author, selected mode, revision, self-update, remote |
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
values preserved. The start card reuses the composer's height measurement and the 64 px tail token. Stages narrower
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
ten jobs at once: on Windows and on macOS, the lints with the `ui` suite and the `rust` suite on its own, and the
`screen` suite on Windows in six parts (`cargo t --suite screen --shard 2/6`). On CI the runner uses every core; on a person's PC it keeps to half. Each kind of job keeps its own Rust cache,
since each builds different things. The runner deals the scenarios so the parts take about the same time: the slowest
first, each to the lightest part so far, using the CI seconds of every scenario that takes 8 s or more
(`testkit/src/screen/mod.rs`, `SLOW`); any other counts as 5 s. A slow scenario left out of `SLOW` makes its part late (on
2026-10-16 one part took 17 minutes against 8 for the others), and a renamed one fails a test. A push that changes only
words (`docs/` or a `.md` file, except `docs/product-map.md`, which a test reads; `scripts/ci-scope.mjs`) skips the
jobs and is green in seconds, but only when the commit before it is green (R163). CI keeps the Windows screenshots as artifacts. Anything outside the page (tray,
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

### What a run prints

A run prints only what needs a look (R154): each failure with its reason, each pass that measures memory or processor
(R125), and one summary line. A failed Rust test shows its panic from the `file:line` on; a compiler error shows as
`file:line:col: error[code]: message`, with the compiler's suggested fix as one `help:` line under it. A reason longer
than 15 lines keeps its first line and its error lines. Every line, every pass and Cargo's whole output included, goes
to `target/faber-test/run.log`, which CI keeps with the screenshots. `cargo test` runs with `--no-fail-fast`, so one run
lists every failing test. `cargo t --suite lint` runs the two Clippy proofs the same way, and a test keeps them equal to
CI's lines. The `cargo t` alias builds the runner itself with `--message-format=short`, so a broken build of the runner
also prints one line per error.

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

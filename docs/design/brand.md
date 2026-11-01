# Identity and brand

The mark, the app icon and the brand rules.

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
  project with nothing in it. A small [scene](scenes.md) rests beside him on the hairline in the Projects and Catalog empties. The line never names a path through Settings, whose sections change names; the button goes there.
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

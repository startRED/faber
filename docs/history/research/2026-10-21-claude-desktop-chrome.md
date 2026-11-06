# Claude Desktop 2.37337.1.0: app chrome behavior spec (clean-room, behavior only)

Source: shipped bundle only (E:\Dev\re-corpus\claude-desktop-2.37337.1.0). Numbers are measured from shipped CSS and
logic, wording is mine. Terms: "page" = the window background behind cards; "card" = inset rounded surface.
Light/dark pairs are written light / dark. All values px unless noted. `rem` scale is 1 at default zoom.

## 1. Left sidebar

Model: the sidebar is NOT a column of the layout. It is an absolutely positioned floating card; the main area simply
gets left padding equal to the sidebar width plus gaps. Resizing therefore only changes two numbers.

| Item | Value |
|---|---|
| Default width | 288 |
| Min / max | 200 / 420; max is also capped to (window width - 330), so the main area never goes under 330 |
| Card inset | 8 from window left, 8 from bottom, top = 8 (macOS) or 8 + 36 on Windows (below the caption-button bar) |
| Gap card to main content | 8 (main area left padding = 288 + 8 + 8) |
| Card radius | 8, plus Electron squircle smoothing (system corner smoothing) |
| Card look | solid fill, 1px ring at 10% ink, soft 24px blur shadow at 4-6% (details in section 6) |
| Handle hit area | 12 wide, straddling the card edge, but the 4.5px on the inside (37.5%) is clipped away, so the live hit zone is the ~7.5px in the gap just outside the card; full height |
| Handle cursor | column-resize (also forced on the whole body while dragging, text selection off) |
| Handle visual | nothing at rest. On hover a 3px x 48px rounded pill fades in at the card edge (muted ink, 0.2s fade, 0.1s delay on appear, 0.12s snap when dragging). In the "panels" variant it is a 2px full-height accent line instead. Keyboard focus: pill becomes 3px full height in accent color with a focus ring |
| Tooltip on handle | title "Hide sidebar" + shortcut Cmd/Ctrl+B + second line "Drag to resize"; follows the cursor vertically (tracks Y axis), not hoverable, hidden while dragging |
| Single click (press and release, movement under 3px) | toggles the sidebar closed |
| Double click | does nothing; after a click-to-hide, the next double-click events are swallowed so the sidebar does not flicker back |
| Drag | live: writes the new width straight to a CSS variable and to the main area padding on every move, no React re-render, all transitions off while `resizing` |
| Collapse by drag | raw (unclamped) width below half of min (below 100) hides the sidebar immediately while still holding the mouse; dragging back above 100 restores it. Between 100 and 200 the width clamps to 200 |
| Drag vs click threshold | 3 px of movement |
| Escape / lost mouse / blur during drag | cancels, restores previous width. If the mouse button is released outside the window (button state says up for 200 ms) it commits as if released |
| Keyboard on handle | Left/Right 8px, Shift+arrow 32px; Home = min, End = max |
| Toggle shortcut | Cmd/Ctrl+B, also a button in the header (tooltip "Hide sidebar"); edge handle tooltip when collapsed says "Show sidebar" |
| Persistence | localStorage key `dframe-store` (zustand persist) holds `collapsed` and `sidebarWidth`; width re-clamped to 200..420 on read; a backup copy `dframe-store.backup.v1` is written once. A prepaint script reads it before first paint to avoid a width jump |
| Collapse / expand animation | none on the dock itself: width and main padding change instantly. Only the peek popover animates (below) |
| Collapsed state | sidebar card disappears; a small pill (28 square, 32 in comfortable density) with the sidebar icon stays at the top left (left = 74 traffic-light spacer + 16 on macOS, 8 on Windows). Main header text is pushed right of that pill |
| Peek (hover reveal when collapsed) | pointer within 48px of the left screen edge shows a hint strip (8px wide, popover surface, shadow) sliding in; within 16px and dwelling 100ms opens the peek card. Peek card: 280 wide, max 70vh tall, radius 8, popover shadow, starts at opacity 0, scale .98, 6px up; opens with opacity 0.14s ease + transform 0.18s cubic-bezier(.32,.72,0,1). Closes after pointer leaves for 180ms. An invisible bridge area keeps it open while moving from the pill to the card. Dragging a session over it also keeps it open |
| Edge drag when collapsed | 12 wide strip at the window left edge (4px clipped away); dragging it opens the dock; a click shows the sidebar; cursor column-resize |
| Narrow windows | window minimum is 600 x 400. Viewport max-width of max(700, sidebar width + 330 - 1) counts as "narrow": the sidebar auto-collapses to the pill/peek mode and the docked preference is not lost. At 700 or below there is also a phone-style slide sheet variant (touch) |
| Row geometry (compact default / comfortable) | row height 26 / 30 (desktop comfortable; 32 web); row font 13 / 14; group label font 12 / 13; icon 16 / 20; leading icon slot 24 / 28; gap icon to text 4 / 8; row radius 6 / 8; footer button 24 / 32 |
| Long list | the nav list fades its top 12px with a mask once scrolled; scrollbar sits in an 8px right lane and the lane is measured so rows never shift when the bar appears |

Confidence: high (logic and CSS read directly). Not found: exact hit height of the pill (derives from 48px) is by the
component, the default density the app ships with (user setting).
Where found: ion-dist/assets/v1/shared-styles-C6yuIsHz.css, shared-21-DExJa6Im.js (resize handle, edge peek),
shared-52-DOMr1cuc.js (constants, persisted store), shared-15-J8PhW4f7.js (handle widget), shared-16-BxEk65IO.js (peek timings).

## 2. Panels layout (main column and right side panels)

Three layers, all on the page background (light #fcfcfb / dark #151515):
1. Sidebar card (section 1).
2. Main area = the chat as one more "tile" (no heavy card; it paints on the page/transcript surface).
3. Side panels (browser, terminal, files, tasks, preview) = tiles in a row-based tiling engine next to the chat.

| Item | Value |
|---|---|
| Tile container padding | 8 |
| Gap between tiles | 12 (the gap is the resize handle) |
| Window right/bottom margin | 8 content gutter; sidebar clearance adds on the left |
| Tile surface ("panel") | fill one step above page (surface-2: #fff / #1a1a19), radius 10 (12 in comfortable), 1px ring in the border color (10% ink) plus a tiny shadow `0 1px 2px` at 5%; forced-colors gets a real border |
| Tile header | 32 tall, 4px side padding, 4px gap, window-drag region except on controls |
| Shadows | card shadow only a 1px ring + faint blur; no heavy elevation anywhere in the chrome |
| Boundary handle | hit area = the 12px gap, full tile length; shows a 3px x 56px rounded pill centered in the gap, hidden at rest and fades in on hover in 120ms ease (mid gray 20% ink), turns primary ink while dragging, accent when keyboard focused; "Resize" tooltip |
| Corner handles | small square hit zones at the ends of a handle for moving a junction in both axes (cursor all-scroll) |
| Tile drag handle | top-center 44 x 16 hit zone (cursor move) with a 32 x 3 pill, hidden until hover (120ms) |
| Min sizes | a side tile 280 wide, any tile at least 100 high; the chat tile never squeezes below 320: it keeps a 320 minimum width anchored left and gets clipped instead of reflowing |
| Sizing model | each tile has a flex weight (flex-grow = weight, basis 0, shrink 1); resizing moves weight between the two neighbours; new tile splits its neighbour 50/50; double-click on a handle resets weights to equal. (In the separate split-pane columns system, holding Alt while dragging spreads the change over the neighbours instead of only the two adjacent columns) |
| Layout animation | position changes animate with a spring (stiffness 500, damping 46, near 0.25s, no bounce); animation is off (duration 0) while dragging |
| Not enough room | opening a side panel first opens it as an overlay over the chat's empty right side if the centered chat column leaves space (slides in 0.2s, `cubic-bezier(.165,.84,.44,1)`, from the right by the reserved width). Any click, key or wheel elsewhere (or a window resize) "settles" it into a real docked tile and reflows the chat. If there is no spare room it docks at once |
| Split panes (two sessions) | separate system: columns divided by a 1px line, 12px hit zone (6px each side of the line), 3px click/drag threshold; unfocused pane drops to 75% opacity and 85% saturation (0.12s); focused pane gets a 2px accent inner outline when pane drag is armed |
| Persistence | tile layout (weights) is stored per session in the app's store |

Confidence: high for tokens, spring and handle; medium for exact overlay rules (logic spread across several files).
Not found: a max width for side tiles.
Where found: ion-dist/assets/v1/cc43287c9-CNtlFMhj.js (tile config, overlay), c593f7513-6XeWD_sM.js (tile engine,
handles, spring), shared-18-DScmD_-G.js (gap/padding/min sizes), cf53e1812-BwwaoAVo.js (min size logic),
c4db265df-SIIT6Wj8.js (tile header and body), shared-frame-BjUcQ1Qc.js (panel surface), shared-23-BY6HquaF.js (split panes).

## 3. Built-in browser pane

| Part | Behavior and numbers |
|---|---|
| Container | a normal tile (section 2): surface-2, radius 10, ring + faint shadow. Header 32 tall |
| Header content | left: tab strip (takes the free space). Right group: ghost icon buttons, 4px gap. Tab strip and toolbar share one row (toolbar "toolbar mode" shows address + nav when the pane is a single page) |
| Tab | 24 tall, base width 192 that shrinks (minimum 22 with icon), 2px inner padding, radius 6, label 14px secondary ink, gap 4. Hover background ghost-hover (5% ink / 7.5% white); active tab ghost-selected (10% / 15%). Favicon slot 20 square; close button 20 square radius 4 appears in the tab. Label truncates with an 8px fade mask at the end. Tooltip (title + origin detail) only after a 700ms delay, below the tab |
| Plus / new tab | ghost icon button at the end of the strip (about 40 wide, centered glyph), tooltip "New tab" with shortcut Cmd/Ctrl+T; disabled at the tab cap. With no tabs the strip shows one placeholder tab named "New tab" that cannot be closed |
| Back / forward / reload | ghost icon-only buttons, 24 square, caret-left, caret-right, circular-arrow; back/forward disabled when no history; tooltips "Go back", "Go forward", "Reload" below the button |
| Other toolbar buttons | select element (cursor icon, toggle), open in real browser (arrow out of square), overflow "more" menu (copy URL, screenshot, viewport size, logs) |
| Address field | 24 tall, radius 5, 6px horizontal padding, 14px text, right margin 24; at rest transparent and shows host in secondary ink + path in muted ink; hover bg 5% ink; focus-within bg 10% ink with a 1px inset ring (20% ink); click turns it into a text input with accent caret, Enter navigates, Esc cancels and blurs; placeholder "Enter a URL"; text is LTR even in RTL layouts |
| Content area | fills the tile body, no inner padding, no border of its own. The real page is a native view placed over that rectangle; the renderer reports its rectangle and corner radius to the shell, which applies the same radius to the native view, so the page corners match the card |
| Empty state | centered column, max 420: large globe icon in disabled ink, one line of primary text (max 36ch), one smaller muted line (max 44ch), then secondary action buttons; gaps 4 to 12 |
| Close / expand | tile header close button at the right end (hidden when the host asks), a pop-out button, a menu with tile actions; focus returns to the pane with F6 / Ctrl+F6 to leave the page |
| Fallback view | when the page is not live it shows the last JPEG frame under a backdrop tint |

Confidence: medium-high (tab/toolbar classes read; overall pixel rhythm inferred from the numbers above).
Not found: exact + button width in compact density; icon stroke weights.
Where found: ion-dist/assets/v1/cf580efe9-BNVXQZS1.js (pane, toolbar, address, empty state), cfe7c4b3b-DiLi7yJE.js (tab strip),
c4db265df-SIIT6Wj8.js (tile), cac3d18f6-u0mTfall.js (device wrapper), asar/.vite/build/index.chunk-CHpJbW9_.js (native view bounds + radius).

## 4. Scrollbars

| Rule | Value |
|---|---|
| Global default | every element: standard thin scrollbar (`scrollbar-width: thin`); thumb = border color at 35% alpha on a transparent track (`scrollbar-color`). Applies on Windows too: no separate Windows rule was found |
| Arrow buttons | none specified; the thin standard bar has no arrows |
| Auto-hide | none globally (always the thin bar when the content overflows). Some small scroll areas show the thumb only on hover by switching the color to the border color |
| Fully hidden | utility for hidden scrollbars (width 0, none) on pill rows, tab strips and touch devices |
| Custom pill scrollbars | specific components use webkit pseudo-elements: 4px or 6px wide thumb, fully round, an extra transparent 4px border with background-clip padding to get a slimmer visible pill, thumb color text-muted at 80% (or 20% ink), hover step darker, hover color = border color; hidden below 640px width or on coarse pointers |
| Gutter | main page scroller uses `scrollbar-gutter: stable` on the main route so the transcript does not shift; sidebar nav keeps an 8px lane (section 1); a fallback sets end padding 12 when the property is unsupported |
| Overscroll | `overscroll-behavior: contain` on sheets and some pickers; not set globally |

Confidence: high for the global rule, medium for per-component pseudo rules.
Where found: ion-dist/assets/v1/c6a992d55-CNGsCU46.css (lines ~1330, 1523, 13342-13361, 52787-52860), shared-styles-C6yuIsHz.css.

## 5. "Not a web page" details

| Aspect | Rule |
|---|---|
| Cursor | in the desktop shell, every button, role=button, menuitem, tab, option, switch, checkbox, radio and in-app link shows the plain arrow (default), never the hand; only real text links (inline links styled as links, external links) keep pointer. Disabled stays default. Text areas show the text cursor; zoomable images zoom-in. A single design token flips this per platform (`desktop` = default, `web` = pointer) |
| user-select | the sidebar card and window drag strips are non-selectable; every button is select-none; the resize handle turns off selection on the whole body while dragging. Content (messages, code, path text in the address bar when read-only) stays selectable. No global user-select: none |
| Drag regions | drag strips (window move) are marked as drag regions and every interactive element inside is automatically no-drag (links, buttons, inputs, tab, checkbox, contenteditable...). Title bar zone 36 tall on Windows (caption buttons overlaid, transparent, symbol colors #c2c0b6 dark / #3d3d3a light), traffic lights inset on macOS (74px spacer); content header row is 48 |
| Window frame | main window: hidden native title bar, native caption overlay on Windows, minimum 600 x 400, window background color matches the page (#fcfcfb light / #151515 dark) so there is no white flash. The native web view of the main window gets 8px corner radius (0 when maximized or fullscreen). The app paints its own page background under the cards |
| Focus ring | keyboard focus only (:focus-visible). The browser outline is made transparent (kept 2px for forced-colors) and replaced by a three-layer shadow: 1px inner gap in page color, 1px accent ring, 6px soft accent glow; for inputs the same. Mouse focus on inputs removes the ring and keeps the normal 1px border. Outline color fallback is accent |
| Tap highlight | transparent everywhere |
| Text rendering | grayscale antialiasing (`-webkit-font-smoothing: antialiased`), `text-rendering: optimizeLegibility` in shell windows; geometricPrecision on a few glyph-sensitive spots; system font stack fallback, custom UI font optional (user setting: default, system, dyslexic) |
| Selection color | not set; the OS/Chromium default is used (only transparent selection while a rich-text node is selected) |
| Image / link drag | no `-webkit-user-drag` rule found; links use app-handled drag start; native drag-drop navigation is disabled in the shell (`navigateOnDragDrop: false`) |
| Context menu | not found as a CSS/JS suppression; the shell uses its own menus for chrome and tiles (pointer-down with the right button on tile headers is prevented so the pane does not steal focus) |
| Hover look | always a flat tint, never an outline: ghost hover = 5% ink (light) / 7.5% white (dark); selected = 10% / 15%; secondary buttons 5% / 14%; transition 60ms ease-out (`cubic-bezier(.165,.84,.44,1)`) on background, color and shadow; rows radius 6 (8 comfortable) |
| Tooltips | custom, with shortcut keycaps and optional second line; hover delay 700ms for long labels |
| Misc | `contain: layout` and `isolation: isolate` on panes and sidebar; native overlay scrolling not used; reduced motion honored by both the media query and an in-app switch (`data-reduce-motion`) |

Confidence: high for cursor, drag region, focus ring, window options; medium for the "not found" lines.
Where found: shared-styles-C6yuIsHz.css (cursor, selection), c6a992d55-CNGsCU46.css (drag, focus, preflight),
asar/.vite/build/index.chunk-CHpJbW9_.js (window options, title bar overlay), asar/.vite/renderer/main_window/assets/MainWindowPage-BD9CWQ83.css (tokens).

## 6. Radius scale and surface tokens

Radius (default compact density / comfortable density):

| Use | Compact | Comfortable |
|---|---|---|
| Base control radius (buttons, inputs, tabs, rows) | 6 | 8 |
| Small (checkbox, close chip) | 5 / 4 | 5 |
| Card (menus, message cards) | base + 4 = 10 | 12 |
| Panel / tile | 10 | 12 |
| Sidebar card and framed cards in the shell | 8 (fixed) | 8 |
| Composer | 12 | 14 |
| Pills, dots | fully round | fully round |
| Control heights | 24 (xs 20, sm 20, lg 28) | 32 |
| Spacing steps (xs/sm/md/lg/xl) | 4/6/8/12/20 | 6/8/12/16/24 |

Surface steps (shell cards, `--df-z*`):

| Token | Light | Dark |
|---|---|---|
| Page behind cards | #fcfcfb | #151515 |
| Sidebar and card fill | #ffffff | hsl(0 0% 14.9%) = #262626 |
| Tile (panel) fill (design system surface-2) | #ffffff | #1a1a19 |
| Popover / menu (surface-3) | #ffffff | #20201f |
| Deepest window backdrop (surface-0) | #f9f9f7 | #0b0b0b |
| Hover / selected on sidebar | 5% / 10% ink | 7.5% / 15% white |
| Line (border) | 10% ink | 10% white-ish (alpha-2 of the inverted ink) |
| Strong line | 20% | 20% |
| Card shadow | `0 0 0 1px` 10% ring + `0 4px 24px` at 6% | same ring + 24px blur at 6-8% |
| Float shadow | `0 0 0 .5px 5%`, `0 2px 8px 4%`, `0 12px 32px 3%` | ring 6%, `0 2px 8px` 30%, `0 12px 32px` 25% |
| Pop shadow (peek, menus) | `0 0 0 .5px 6%`, `0 4px 12px 6%`, `0 16px 40px 8%` | ring 8%, `0 4px 12px` 40%, `0 16px 40px` 50% |
| Hairline | 1px on 1x screens, 0.5px on 2x, 1/3 px on 3x | same |

Ink scale (warm gray, light mode): #fcfcfb, #f9f9f7, #f0efec, #e1e0d9, #c3c2b7, #a5a49a, #898781, #6d6b67, #52514e,
#383835, #20201f, #151515, #0b0b0b. Text: primary near-black, secondary gray-600, muted gray-400 (dark: #f0efec, #c3c2b7, #898781).
Accent is the Claude clay #d97757 (emphasized #c6613f).

Confidence: high (tokens read from the shipped design-system CSS).
Where found: asar/.vite/renderer/main_window/assets/MainWindowPage-BD9CWQ83.css (tokens), ion-dist/assets/v1/shared-styles-C6yuIsHz.css
(`--df-*` card tokens), c6a992d55-CNGsCU46.css (utility mapping).

## 7. Motion

| What | Duration / easing |
|---|---|
| Hover tints, button state, link underline | 60ms, `cubic-bezier(.165,.84,.44,1)` ("ease-out") |
| Row hover / small snap changes | 0.12s, same curve (or `cubic-bezier(.32,.72,0,1)` "snap") |
| Hover pills on handles, drag grips | 120ms ease |
| Focus shadow | 60ms |
| Base fades (reveal, toast-like) | 0.2s ease-out |
| Sheets / big slides | 0.3s; slowest 0.45s |
| Sidebar dock / undock | no animation (instant); resize live with transitions off |
| Sidebar peek popover | opacity 0.14s ease, transform 0.18s `cubic-bezier(.32,.72,0,1)`; from scale .98 and 6px up |
| Edge hint strip | opacity 0.14s, slide 0.18s same snap curve |
| Side tile open as overlay | 0.2s ease-out slide |
| Tile reflow | spring, stiffness 500, damping 46 (about 0.25s, no overshoot); 0 during drag |
| Pin section reveal | height and opacity 0.2s `cubic-bezier(.32,.72,0,1)` using intrinsic height animation |
| Pane focus dimming | filter 0.12s ease-out |
| Menus and popovers open | opacity + scale from .98, 0.12s snap curve; larger surfaces 0.2s |
| Overshoot (chips, pop-in) | `cubic-bezier(.34,1.3,.64,1)`, 0.22-0.3s, used sparingly |
| Reduced motion | all of the above set to none via the media query and the in-app switch |

Confidence: high.
Where found: shared-styles-C6yuIsHz.css, c6a992d55-CNGsCU46.css, c593f7513-6XeWD_sM.js, MainWindowPage-BD9CWQ83.css.

## Shell window options (asar, Electron main)

- Main window: `titleBarStyle: hidden`, `titleBarOverlay` on (Windows draws caption buttons over the web content, height 36
  at 100% zoom, transparent background, symbol color follows theme), traffic lights positioned by the shell on macOS,
  minimum 600 x 400, `backgroundColor` set to the page color (#fcfcfb or #151515), not transparent, no vibrancy found
  for the main window.
- Helper windows (quick entry): transparent, frameless, no shadow, no rounded corners from the OS; the About window uses
  `hiddenInset` and is fixed 320 x 428.
- Artifact popout windows: normal frame, background color matched to theme, min 320 x 240, menu bar auto-hidden.

Where found: asar/.vite/build/index.chunk-CHpJbW9_.js (lines ~126960, ~263870).

## Rebuild checklist (the 12 things that create the feel)

1. Page color darker than every card; all chrome as inset cards with 8px margins and 8px gaps.
2. Sidebar is a floating 288 card (200..420), content padding follows it; no width animation.
3. Handle only appears on hover as a thin pill; click toggles, drag resizes live, below half of min it hides.
4. Cursor stays an arrow on buttons, links in chrome, tabs, rows.
5. Only focus-visible rings, made of a ring + glow, never the browser outline.
6. 60ms hover tints with 5% / 10% ink steps (7.5% / 15% in dark).
7. Thin scrollbars, transparent track, translucent thumb, stable gutter.
8. Chrome text is not selectable; content text is.
9. Tiles 12 apart with the gap as the handle; spring reflow; chat never under 320, side tiles at least 280.
10. Side panel opens as an overlay when there is spare room, docks on the next interaction.
11. Window and page background identical to avoid flashes; title bar overlay 36 on Windows.
12. Hairline borders at 10% ink, soft 24px shadow at 4-6%, radii 6 / 8 / 10.

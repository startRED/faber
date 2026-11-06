# Faber screen audit (read-only), 2026-10-16, branch phase-1-base

Scope: `ui/*.tsx|ts`, `ui/styles.css` (934 lines), `ui/fonts.css`, `src-tauri/src/themes.json`, plus the two screenshots.
Nothing was edited and nothing was run; every finding is from reading code. "Verify" marks something that needs a live check.
Line numbers are from `grep -n` on the working tree (uncommitted changes included).

Ranked by impact. Severity: H = breaks a rule or costs real CPU/memory, M = two ways of doing one thing or a visible rough edge, L = isolated polish.

---

## D. The Settings "overlapping sections" suspicion (answered first)

**D1. Not a stuck bug: it is the section-switch exit copy caught mid-fade. But the design behind it is wrong (H, by the project's own rule).**

Evidence that the screenshots are mid-transition:
- `target/ux-review-2026-10-16/receipts.json`: 05 is shot 187 ms after the click, 07 is shot 128 ms after, 08 is 123 ms after the previous click. The exit lasts 180 ms and the arrival 220 ms, so each capture lands inside the crossfade. 03 and 04 (shot after 125 ms and 690 ms) are clean.
- Cause: `ui/Settings.tsx:119` puts `ref={animateList}` on `.s-main`, and the sections are a `<Switch>` (`Settings.tsx:124`). `animateList` (`ui/motion.ts:~112-185`) sees the old children removed, deep-clones up to 12 of them into a fixed-position shadow layer (`exitSurface`, `motion.ts:8-27`) at their old rectangles, and fades them out over `--motion-exit` while the new children fade in from opacity 0 with a stagger (`arrivals++ * --motion-stagger`, `motion.ts:~176`). Two text layers at the same coordinates at roughly 50% each is exactly the garbled text in 05 (Appearance ghost over Accounts) and 07 (Accounts ghost over Usage; the two `hint` lines overlap).
- It is not stuck: ghosts are removed in `animation.finished` (`motion.ts:~160`) and 08 shows Usage still fading 120 ms later.

What is wrong anyway:
- `docs/design.md` "Motion" and `.claude/skills/faber-ui/SKILL.md` section 1 say a tab or row switch changes instantly or with a 150 ms fade at most, never travel, and "text the user is reading or acting on never moves for style". A Settings section switch is a tab switch. The crossfade of two full-text layers is the one case that looks broken in the middle.
- Fix: do not give `.s-main` exit ghosts (skip `ref={animateList}` here, or have `animateList` skip ghosts and stagger when `root.matches('.s-main')` and only fade the new content in over `--dur-fast`). Same for `.rc-body`/Running if the owner agrees.
- Related real defects found while reading it:
  - **D2 (M)** `Settings.tsx:119`: the `.s-main` scroller is the same element for every section and nothing resets `scrollTop` (no `scrollTop`/`scrollTo` call in `ui/Settings.tsx`). Scroll General down, click Usage or Privacy: the new section opens scrolled (clamped, not zero) and can look like it is missing its top.
  - **D3 (L)** `motion.ts:~124-141`: ghosts are `position: fixed` copies sized from the pre-switch rect and are not clipped to the stage (`overflow`/radius of `.s-main`), so a ghost of content that was scrolled partly out of the stage draws over the stage margin and shadow until it fades. It also deep-clones the whole Appearance subtree (six `.mini` previews with grain) on leaving Appearance.
  - **D4 (L)** Entering Appearance triggers six `api.previewLook` IPC calls (`FirstRun.tsx:68-80`, one per `Mini base=`) while the 220 ms arrival is playing; the thumbnails pop in with default look first, then re-skin. Not an overlap bug, but it is the visible stutter on that switch.

---

## A. Visual inconsistency

**A1 (H) `.btn.secondary` has no CSS.** Used 9 times (`QuotaCard.tsx:24,25,28,31,34`, `Settings.tsx:261,289`, `Setup.tsx:186,204`); `styles.css` defines `.btn`, `.btn.primary`, `.btn.ghost`, `.btn.big` (lines 368-374, 543-544) and nothing for `secondary`. The author intended a role that does not exist, so those buttons are identical to default `.btn`. Decide: delete the class, or define it (and use `ghost` for the quiet role). Today there are four names for three looks.

**A2 (H) No `color-scheme` anywhere** (`grep color-scheme` finds nothing in `ui/`, `index.html`, `themes.json`, `look.rs`). Every native part follows the OS: the unstyled `<input>` of `QuotaCard.tsx:50` (`.plan-form input` has no rule), `.env-tray input` (`styles.css:911`, no `border:0`, no background, so a UA border plus the inset ring), `<input type=color>` (`Appearance.tsx:160`), form-control autofill, the fallback native `<select>` popup where `appearance: base-select` is unsupported (`styles.css:~724-735`). Dark looks on a light-mode OS (and the Morning preset on a dark-mode OS) get white or black native chrome. Fix: the look engine writes `color-scheme: dark|light` on `<html>`.

**A3 (M) Seven "pick one of N" widgets, each its own CSS:** `.seg-ctl button` (43-47), `.choices button` (444-449), `.preset` (428-436, Settings looks), `.tcard` (569-579, first-run looks, same job as `.preset`), `.pick` (548), `.sizes button` (601-604), `.choice2 button` (605-609), plus `.ask .q-option` (959-961) and `.dot`. They differ in pressed ring (1px vs 1.5px, `--s3` fill vs `--accent-soft` fill vs `--accent` ring), height, radius and hover. One `.opt` pressed-state recipe would cover all of them. `.preset` vs `.tcard` should be a single component with a size prop.

**A4 (M) Seven text-input styles:** `.name-in` (inset ring, 32px), `.hex` (28px), `.env-tray input` (32px, no reset), `.ny-custom input` (850: real `border: 1px solid`, `--bg` fill, the only bordered one), `.ask .q-other` (958), `input.rename` (892: `--s3` fill, accent ring), `.search input`, `.proof-draft` (837), and the unstyled plan input (A2). Heights 28/30/32; rings by `box-shadow` except one by `border`. One `.field` class.

**A5 (M) Control heights are not a scale:** `.btn` 32, `.ap-bar .btn` 36 (styles `.ap-bar .btn`), `.btn.big` 40, `.send` 34, `.pill` 28, `.chip` 26, `.chip.choose` 30, `.topbar .chip` 30, `.run-btn` 32, `.seg-ctl button` 26 then overwritten to 30 two lines later (44 and 47, line 44's `height: 26px` is dead), `.search` 32, `.newchat` 38, `.item` 28/34 (`--dens`-aware) vs `.btn`/`.iconbtn` 32 (not `--dens`-aware). Define 3 control heights (28/32/40) as tokens and use them. Settings buttons in one row (`.btn` 32 beside `.seg-ctl` 36 total beside `.name-in` 32) do not line up.

**A6 (M) Font sizes: 8 different offsets around `--fs-ui`** (counted in `styles.css`: -1 ×51, -2 ×21, -1.5 ×19, -0.5 ×17, +0.5 ×6, +1 ×3, -3 ×2, -2.5 ×2) and the theme defines `--fs-xs` 12px and `--fs-sm` 13px (`themes.json:43-44`) that **no rule uses** (0 hits). Half-pixel steps (-0.5, -1.5, -2.5, +0.5) on a 14 px base render at fractional sizes and differ per text-size setting; design.md says "base plus or minus a few pixels". Collapse to 3 UI sizes (xs, sm, ui) using the two tokens that already exist.

**A7 (M) Fixed `px`/`em` text that ignores the text-size setting (13-18 px):** `.proof-list code` and `.proof-draft` 13px (835, 837), `.task-card pre` 13px (864), `.flow-label` 12.5px (690), `.m-in`/`.m-btn` 10.5px (598-599, only inside the thumbnail, acceptable), `.pace, .pace-projection, .waiting-label` use `.85em` (764, em compounds with the parent's size), `FirstRun.tsx:235` inline `font-size` px. The first three are on screens a person reads.

**A8 (M) Hard-coded shadows bypass `--lift-shadow`:** `.work > .runcol` `-18px 0 40px -22px rgba(0,0,0,.7)` (244), narrow `.side` `18px 0 40px -20px rgba(0,0,0,.7)` (271); `.tour-ring` `0 0 0 200vmax rgba(0,0,0,.6)` (623); scrims `rgba(0,0,0,.5/.6)` (273, 621). On the light Morning look a pure-black 0.7 shadow is heavy; the look tokens have no say. Add `--overlay-shadow` and `--dim` tokens.

**A9 (M) Hard-coded radii ignore the Corners setting and the sharp looks** (Workshop/Workbench set `--r-xs` 4, `--r-pill` 6-8, `--r-stage` 0): `.item kbd`/`.newchat kbd` 5px (70), `.seg-ctl .sq` 3px (464), `.sw2` 9px (469, a switch that should follow `--r-pill`), `.meter` 3px (519), `.m-side i` 3px (582), `.m-flow` 14px (590), `.g-caret` 1px, `.live` 1px (square mark, intentional). `.dot` uses `--r-pill`, so on Workshop (6px) the colour swatches become rounded squares while the neighbouring `.sw2` stays a pill: the same page mixes both.

**A10 (M) Two timing token families.** `:root` defines `--motion-enter/-exit/-fast/-ease-out/-ease-in/-spring` (styles.css:7-13) next to the theme's `--dur-fast/--dur-ui/--dur-arrive/--ease/--ease-io`. Components pick either at random (`.item` uses `--dur-fast` at line ~62 and `--motion-fast` at line ~706, the later rule wins; `:is(button,select,.plus,.x)` at ~716 re-declares transition for every button and overrides component-level ones of equal specificity). Literal `120ms` at 959 (`.ask .q-option`) and 959's duplicate `@media (prefers-reduced-motion)` at 965 duplicates the global rule at 739-741. SKILL.md says "a second set of durations or curves is a defect": this is two sets.

**A11 (M) Card surfaces drawn six ways:** `.accounts li` (inset `--line-2` on `--s1`), `.setup-card` (`--card-bg` + `--card-shadow`), `.quota-card` (765: `border:1px solid`, no radius, `margin:12px 0`, `.btn{margin:4px}`; looks like it came from another app), `.afile`/`.file` (`--s1` + inset `--line-2`), `.ri` (`--card-bg` + `--card-shadow`), `.pvbox`, `.contrast div`, `.tools span` (inset `--line`). Radii 8/12 mixed. Pick `--card-bg/--card-shadow` for "object" and `--s1 + --line-2` for "field", and use only those.

**A12 (L) Icons by text glyph:** `App.tsx:692` toast dismiss and `Sidebar.tsx:166` delete use the literal `×` while `Recents.tsx` `.rec-x` and `Running.tsx` use `<Icon name="close">`. `locales/en.ts:209` `'← Back'` bakes an arrow into the string although `Icon left` exists. Different weight and baseline from the line icons.

**A13 (L) Tokens that exist but are bypassed by literals:** `#08080a/#f3f2ee/#e9ede7/#1c2924` (15-17, first frame; intentional and documented), conic `#f3f0e8…` (457, decoration, acceptable), `#000` in masks (fine). No stylelint-style test enforces any of this (`tests/ui/styles.check.ts` only checks specific rules). Add a lint that fails on new hex/rgba/px-radius/`ms` literals outside an allowlist.

**A14 (L) Section widths and rhythm in Settings are not one system:** `.s-main > *` max 880 (414), `.accounts` 520, `.usage-v` 600, `.facts` 560, `.row .toggles` 480, `.name-in` 280. General mixes `.row` (label left 160px, `Settings.tsx:74`) and `.ctl` (label above, `controls.tsx`) for neighbouring controls: language/name/hooks use `Row`; permissions/web search/summary/verbosity use `Segmented`/`Choices`. Two row grammars in one section.

**A15 (L) `.usage-v`/`.urow` number format:** `Settings.tsx:280` and `Thread.tsx:65` use `toFixed(4)` (dot decimal), so pt-BR shows "US$ 0.0000" (visible in `07-usage.png`). Use `Intl.NumberFormat(lang(), {style:'currency'...})`; also covers known incoherence 12. `.urow`'s 4-column grid also receives extra children (`.pace-projection` spans via `grid-column:1/-1` at 771, `.note.age` and `.note.uncounted` do not all span), so rows with a reading can misalign columns (verify with a real reading).

---

## B. Structural duplication (and the product-map "Known incoherences" 7-17)

**B1 (H) `ui/Projects.tsx` (917 lines, 42 KB) is dead in production.** `App.tsx:579` `needsProject = area==='project' && !project()` and `App.tsx:978` mounts `<Projects project={project()}>` only inside that branch, so `props.project` is always undefined there. The start card (`Projects.tsx:485`), its own goal `<textarea>` (514), the proof sheet, the mission board, `LocalBuild`, the project `Hole`, ~77 CSS rules ("Projects board and start card", styles.css:774-~950) and `ui/LocalBuild.tsx` can never render; only `tests/ui/projects.check.ts` mounts them directly. product-map.md:16 confirms "no way to start one on screen". Cost: parse/bundle weight (591 KB single JS chunk), a 30 s `setInterval` IPC that still runs on the "Open a project" screen (`Projects.tsx:123`, `api.localUpdateState`, plus a `focus` listener), and every reader of the code meets a second, divergent home. Either delete the branch or gate it behind a dev flag and lazy-load it.

**B2 (M) Known incoherences 7-17, status in code today:**
- 7 (Chat vs Projects differ): **mostly resolved on the live path**: a project's new conversation now uses `Welcome` + the shared `Composer` (`App.tsx:952`, `Welcome.tsx:44`). What remains is only in the dead branch (B1): own textarea, 3-row pick-up (`Projects.tsx:411`, `h3`) vs 4-row (`Welcome.tsx:81`, `h2`). Update the map line.
- 8 (model choice in four places): **still true**: composer, `Thread.tsx` per-answer `<select class="redo">` (~line 260, one `<select>` plus N `<option>` per answer), the `nudge` button, and the right-click Redo (`Thread.tsx:~188`, silently uses `picked()` at the caller). Owner decision still open.
- 9 (two Open-a-project screens, three folder labels): **still true in text** (`recents.choose` "Choose a folder", `set.chooseFolder` "Choose folder", `side.addProject` "Add project folder"), but `App.tsx:1033-1046` (the `Empty` branch, `btn` secondary) is unreachable now because `Empty` is rendered only when `!needsProject()`; the reachable one is `Projects.tsx:444-455` (`btn primary`). Same role, two button styles, one of them dead. Remove the dead branch.
- 10 (two right-click menus): **still true**: `Recents.tsx:164-181` + `styles.css:912-914` `.rec-menu` (different z-index 30 vs 60, `--s3` vs 96% `--s2` with blur, `animation: arrive` vs `cmenu-in`, no keyboard arrows, own Esc/pointerdown listeners at `Recents.tsx:41-52`) against `ContextMenu.tsx`.
- 11 (three ways to confirm a delete): **still true**: Sidebar `×` two-click (`Sidebar.tsx:133`), the menu's `confirm` field, Recents Remove with none.
- 12 (money/quota formats): **still true** (see A15; plus `Math.round(x*100)%` formatting in four files).
- 13 (three meanings of "update itself"): the Local build card part is now dead code (B1), so only two meanings are reachable.
- 14 (name and text size asked two ways): **still true**: `first.name.title` "How should Faber call you?" vs `set.name` "Your name"; first-run `SIZE_PX` 13/15/18 buttons (`FirstRun.tsx:148`) vs slider (`Appearance.tsx:218`). The label key `set.textSize` is now shared.
- 15 (first-run answers that cannot change): not re-verified.
- 16 (account entry labels): **still true**: `empty.noAiButton` "Connect a subscription", `set.openSetup` "Manage accounts" used on two Settings buttons (`Settings.tsx:261`, `289`), "Codex" vs "ChatGPT" text.
- 17 (chat working folder in global Settings, dead when opened from Projects): **still true**: `Settings.tsx:~203` renders the label and hint always but the control only `when props.onChatFolder`, which `App.tsx` passes only for `area()==='chat'`.

**B3 (M) Two `Segmented` components:** `ui/controls.tsx:9` and a byte-equivalent copy at `ui/FirstRun.tsx:129` (only the `value` type differs). Delete the copy.

**B4 (M) Two disclosure cards with copy-pasted CSS:** `details.activity` (`styles.css:~330`) and `details.agent` (`styles.css:~343`) differ only in a margin and a mark; both `summary` rules are the same 6 declarations. One `.disclosure`.

**B5 (M) Two "pick a look" cards:** `.preset` (Settings) and `.tcard` (first run), both render a live `Mini` of each theme with different radio/ring language. Same component, two sizes.

**B6 (L) Row/list grammars:** sidebar `.item`/`.crow`, `.rrow` (pick up), `.ri` (running card), `.rec` (recents), `.tools span`. Hover treatments differ (`.item` background + `filter: brightness`, `.rec-main` accent text, `.rrow` child transforms).

**B7 (L) `Recents` is mounted from three places** (`App.tsx` Empty, `Projects.tsx`, `Settings.tsx:211`) with the same `createResource(api.recents)`, so each mount re-reads and re-judges up to five folders on the engine; no shared cache.

---

## C. Performance and memory

**C1 (H, CPU, long answers) Streaming re-parses the whole answer 30 times a second.** `engine/src/engine.rs:1699` emits the *full* `Message` every 33 ms; the screen then runs `marked.parse` + `DOMPurify.sanitize` + a regex over the whole `finalText()` and assigns `innerHTML` (`Thread.tsx:30`, `:238`) on every tick, even when only `tools`/`thinking` changed (the binding re-runs because `m()` is a new object each time, and Solid's `innerHTML` assignment has no equality check). Cost is O(n²) in answer length and it throws away the DOM under the user's selection and any open code block scroll on every tick. Same for each `agent-report` and `narration` block (`Thread.tsx:~113, 211`). Fix: memoise `html()` per text string (`createMemo` on `finalText()`), and for the streaming tail render only the last (open) block, keeping closed blocks as stable DOM.

**C2 (H, input path) Per-keystroke layout thrash and storage write.** `Composer.tsx:~150` (`onInput`) sets the draft signal, which (a) runs `keep()` -> `localStorage.setItem` synchronously every key (`unsent.ts:59`), and (b) `resizeComposer` (`composer-size.ts`) reads `getBoundingClientRect`, `getComputedStyle`, writes `style.height='0px'`, reads `scrollHeight`, writes again: two forced layouts per key, on the one control the user types in all day. Debounce the storage write (250-400 ms, flush on blur/`pagehide`/send) and only re-measure when `scrollHeight` could have changed (newline, wrap) or throttle to one rAF.

**C3 (M) `animateList` observers and listeners multiply.** `animateList` is applied to ~14 roots at once in a normal session (`.app`, `.work`, `.col`, `.side-scroll` (subtree), `.projects` (subtree), `.list` (subtree), one `<section>` per chat group, one `.proj-chats` per expanded project, each running `.ri`, `.rc-body`, `.ritems`, `.s-main`, `.welcome-grid`, `.resume`), and each root adds a `MutationObserver`, a window `resize` listener, and capturing `scroll` + bubbling `transitionend` listeners (`motion.ts:246-248`). Every row hover fires `transitionend` events that bubble to each ancestor root, and each `capture()` calls `getBoundingClientRect` plus `getComputedStyle(el).display` for every child of that root (`children()`, `motion.ts:~104`); thread scrolling does the same on `.app` and `.work` through capture-phase `scroll`. One DOM change in the sidebar triggers up to three nested observers that each re-measure. Impact: a few hundred extra style/layout reads per interaction, more with a long chat list. Fix: skip `capture()` when `animations.size===0` and the event is not from a direct child; share one `window` resize listener; do not nest roots (the `subtree: true` root already covers the `section`/`.proj-chats` roots).

**C4 (M, possible visible flicker) `refresh()` replaces state wholesale** (`App.tsx:175-179` -> `refreshed()` at `appearance.ts:30`, no `reconcile`). On every engine `list` event (`App.tsx:343`) `state().conversations` / `projects` are fresh objects, so `<For each={...}>` in `Sidebar.tsx` (`:231, 260, 277`) rebuilds every row, and `animateList` then plays arrivals plus up to 12 exit ghosts for rows that did not change. Key by id (`reconcile(..., {key:'id'})` or `<For>` over ids) so unchanged rows keep their nodes. Verify with a rename during a running answer.

**C5 (M) `backdrop-filter` and giant blurs.** `.work > .runcol`, `.scrim`, `.coach` (704) blur 8px behind a 94-96% opaque panel: nothing visible is blurred but the compositor still renders the backdrop every frame while the thread streams behind the overlay running column (below 1100 px). Same on `.cmenu` (883) and `select::picker` (728). Drop the blur where the fill is >=94% opaque (no visible gain) or keep it only on the scrim. `.amb-beam .beam { filter: blur(30px) }` (662) on a 760x680 rotated layer is the biggest static raster (documented at ~10% of a core on Dark Room home); acceptable but it is the first thing to cut if the budget tightens.

**C6 (M) Hover `filter: brightness(1.06)` on every row** (`:is(.item,.rrow,.ri):hover`, 708; also `.btn.primary:hover`, `.send:hover`). A filter promotes the element to its own layer on hover and is applied over text; the same rows already change `background` on hover (62-65), so it is a second, duplicate hover effect and contradicts "row hover: no travel, colour fade only". Remove the filter or the background change.

**C7 (M) Per-answer controls:** every answer (`Thread.tsx:~260`) renders a `<select class="redo">` with one `<option>` per model; `content-visibility: auto` (291) keeps paint cheap but not DOM, and a 200-message thread carries ~2,000 `option` nodes plus `Show`/`details` per message. Render the redo control lazily on hover/focus of the answer, or as one shared menu.

**C8 (M) Images as base64 strings in the DOM and in state:** `Thread.tsx:~315` and `Queued` set `src={`data:${mediaType};base64,${base64}`}` on `<img>`: the full base64 string lives in the signal, in the attribute and in the decoded bitmap. A few photos are tens of MB of strings. Use `URL.createObjectURL` (and revoke), or store a thumbnail only.

**C9 (M) Timers that run while nothing is visible:** `App.tsx:354` one-minute interval and `api.setClock` IPC (also with the window hidden); `Projects.tsx:123` 30 s IPC poll (B1: for nothing); `Running.tsx:25` 15 s interval created in the component body, not gated on `windowHidden()`; `Ambient.tsx:143` 10 Hz interval that does `querySelectorAll('.dust i, .amb-glow')` every 100 ms instead of caching the nodes. The last is gated by `breathes()` (hidden/unfocused), the first three are not. Gate on `document.hidden`; cache the NodeList.

**C10 (M) No code splitting:** `dist/assets/index-*.js` is 591 KB in one chunk (`vite.config.ts` has no `manualChunks`; the only dynamic `import()` is the test harness). First run, tutorial, Appearance, Settings, Hole (535 lines), Projects (B1), `marked` + `DOMPurify` and every locale are parsed before the first frame. `marked`/`DOMPurify` could load on the first answer, and Settings/Appearance/FirstRun/Tutorial/Hole on first use. Locales: check that only the active language is loaded (`i18n.ts`); all five are statically bundled if imported eagerly.

**C11 (L) Fonts:** all fonts in `fonts.css` are referenced by a theme or the heading option, so nothing is unused in the CSS. But (a) the Fontsource `@import`s emit both `.woff` and `.woff2` files for IBM Plex, Barlow and Atkinson (`dist/assets` holds both, ~2.4 MB total), doubling disk size for no gain on WebView2 (woff2 only); (b) Archivo `wdth` axis files are ~90-100 KB each x4 (normal + italic, latin + latin-ext) and `latin-ext` is requested only when a glyph needs it, fine. Memory impact is small; installer size impact is real. Keep only woff2 by importing the `*-latin-*.css` variants that reference woff2 or by trimming with a Vite plugin.

**C12 (L) Universal-selector motion rules** (`styles.css:739-741`): `:is([data-motion='reduced'],[data-motion='none']) *` and `*::before/::after` match every element and force an ancestor walk with an attribute test for each style resolve; line 734 is a redundant subset of it. Set the motion level once as a custom property (`--anim: 0`) or use `html[data-motion=...] *` only inside a stylesheet toggled by class.

**C13 (L) Preview thumbnails:** Appearance mounts 7 `.mini` trees (six presets + the preview), each with a `.grain` SVG `feTurbulence` data-URI background (rasterised per element) and `zoom: 0.3` on a 340x250 box (`styles.css:430`, non-standard `zoom` re-lays out at full size). Fine on open; do not keep them mounted when leaving the section (they are not, `Switch`/`Match`), and prefer pre-rendered PNG thumbnails if the section ever feels slow.

**C14 (L) Redundant resize observers streaming:** `App.tsx:904` and `:915` attach two `ResizeObserver`s that both set `scrollTop = scrollHeight` on every height change of `.thread` and `.col` while streaming; the second subsumes the first. Each assignment forces layout.

**C15 (L) `.tour-ring` `0 0 0 200vmax` spread shadow** (623) transitions a transform on a ~2,000x2,000 px shadow: rare, but it is the heaviest single raster in the app. Use the existing `.tour-dim` plus a clip-path hole.

---

## Things that are already right (do not "fix")
- Hole and Ambient stop on hidden/unfocused and with reduced motion (`Hole.tsx:208`, `Ambient.tsx:94`, `styles.css:746-748`); idle ghost timers end with their owner (`idle.ts`).
- Messages use `content-visibility: auto` with intrinsic sizes (`styles.css:291-292`), `.thread` uses `contain: strict` (156).
- `will-change` is used only on the dust/glow layers (668, 676), as the skill allows.
- `transition: all` appears nowhere; shadows ringing uses transparent-border-safe inset rings; `Icon` uses `currentColor`.
- Only `Recents.tsx` adds a window listener at component-body level, and it does clean up.

## Suggested order of work
1. D1 + A1 + A2 (one afternoon, high polish value): no exit ghosts on Settings sections, define or remove `.btn.secondary`, write `color-scheme`.
2. C1 and C2 (the two paths a person feels on every message): memoise markdown, debounce the draft write, throttle composer resize.
3. B1 (delete or lazy-load the dead Projects branch), C4, C3.
4. A3-A6, A10 (token and control unification; add a style lint so it stays fixed).
5. The rest.

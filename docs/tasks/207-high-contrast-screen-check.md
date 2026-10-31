# Windows High Contrast: every screen stays readable and usable

Area: Appearance
Files: ui/styles.css, ui/Appearance.tsx, tests/ui/styles.check.ts, tests/ui/appearance.check.ts, docs/design/looks.md, docs/product-map.md
Depends: none
Size: about 250 changed lines

Why: Windows' contrast themes set `forced-colors: active`, which replaces Faber's colours. `ui/styles.css` has only a few `forced-colors` rules, so cards, the composer border, the quota bars and the selected row can become invisible or unreadable for the people who turn it on.

Build: a list of Faber's controls whose meaning is carried by colour or a background alone (selected row, focus ring, quota bar fill and pace mark, switches, status dots, the permission card's choices) and, for each, a `forced-colors: active` rule using system colours (`CanvasText`, `Highlight`, `ButtonText`, `GrayText`) with a border or an outline so the meaning survives. Drop `forced-color-adjust: none` where it is not needed. No change to the looks outside this mode.

Leave out: new themes and any setting.

Done: a screen check that emulates `forced-colors: active` (see how `tests/ui/styles.check.ts` and the motion checks emulate media) and fails today: each listed control has a visible border or outline distinct from its surroundings, and focus stays visible. Update the page of `docs/design/looks.md` and the map's Appearance row. Cost: CSS only.

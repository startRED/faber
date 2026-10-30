# Replace the flat model list with a menu grouped by AI

Area: Composer
Files: ui/Composer.tsx, ui/ModelMenu.tsx, ui/models.ts, ui/styles.css, ui/locales/en.ts, ui/locales/pt-BR.ts, ui/locales/es.ts, ui/locales/fr.ts, ui/locales/de.ts, tests/ui/models.check.ts, tests/ui/composer.check.ts, docs/design/composer.md
Depends: none
Size: about 400 changed lines

Why: the live look of 2026-10-29, next to T3 Code (`docs/research/2026-10-29-faber-diagnosis/README.md`, "Live look"
and part 3): Faber's model menu is a flat list of 11 names with a scrollbar that opens upward over the headline; T3
groups models by vendor with icons, a search box, the four current models with shortcuts and the older ones folded under
"Legacy models", and shows model and effort in one chip. Verified: the model is a native `<select>` over a chip
(`ui/Composer.tsx` lines 776 to 797) listing `props.options` from `modelOptions` (`ui/models.ts` line 12: "Auto",
three Claude aliases, then every non-hidden model `model/list` returned for the account, `engine/src/codex.rs` near
line 2349), and the effort is a second `<select>` chip beside it (line 804). A native list cannot group, search or
fold, and the platform decides where it opens.

What to build: a menu component (`ui/ModelMenu.tsx`, built on the app's own popover and keyboard patterns: look at
`ui/ContextMenu.tsx` and `ui/Palette.tsx` first) that opens downward when there is room, with "Auto" on top, one
group per connected AI with its mark (`VendorMark`, `ui/Glyphs.tsx`), the current models first, and the older ones folded
under "Mais antigos". Which are current: the models in Auto's table (`PROFILES`, `engine/src/route.rs` line 33) and the
account's default (`ModelInfo.isDefault`), the rest are older; check the rule on the owner's list before choosing it.
A search box shows only when more than eight models are listed. The chip shows model and effort together ("Sonnet 5.5 ·
alto"); picking the chip opens the model menu and the effort is a row inside it (the effort rule, `DEFAULT_EFFORT` and
"a new model starts again at the default" in the `onChange`, stays). Keyboard, screen reader and Esc as the native
select gave them.

Leave out: new models, changing what Auto picks, the permission chip (task `access-choices-say-what-they-allow` may
share the control), model names (`prettyModel`).

Done: screen checks in `tests/ui/models.check.ts` and `tests/ui/composer.check.ts` fail today (there is one `<select>`
with every model); they pass with grouped options, older ones folded, search finding a model by part of its name, the
menu opening downward in a 720 px window and the choice reaching `onChoice` exactly as before. Extends: product-map
"Composer". Update `docs/design/composer.md`. What's-new entry in every language. Walk it at 720 px, longest language,
reduced motion, light and dark. Visual critique before and after. The commit states the memory and processor cost (one
component, no timers). Review by the other vendor (R146) and the coherence review. If the result passes 600 lines, land
the menu first and the one-chip merge of model and effort as a follow-up task.

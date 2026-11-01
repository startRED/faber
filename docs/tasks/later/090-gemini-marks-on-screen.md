# The screen names Gemini wherever it names the AI that answered

Area: Chat conversation
Files: ui/api.ts, ui/Glyphs.tsx, ui/styles.css, ui/Thread.tsx, ui/Running.tsx, ui/Settings.tsx, ui/Accounts.tsx, ui/locales/, src-tauri/src/tray/decide.rs, tests/ui/, docs/design/, docs/product-map.md
Depends: gemini-vendor-variant, agy-turn-session
Size: about 330 changed lines

Why: once a Gemini conversation can run, every place that draws "which AI" must know a third. The mark is a CSS-drawn glyph
chosen by class (`VendorMark` in `ui/Glyphs.tsx` takes `'claude' | 'codex'`), the answer header shows the vendor mark, the model
and its effort, the running column, Settings' usage bars, the account cards, the command palette and the picker list vendors, and
the tray's text names "Claude", "Codex" or "AI" (`stopped_text` in `src-tauri/src/tray/decide.rs`). The product does not use a
vendor's own logo as a feature name (AGENTS.md: vendor names stay plain text), and the marks of the other two are Faber's own
drawings.

Build: the `Vendor` type of `ui/api.ts` (added by `gemini-vendor-variant`) reaches every `claude`/`codex` branch of the screen
(find them with `rg "'codex'" ui` and the `.mk.codex` styles), an original mark for Gemini in the style of the other two (a simple
glyph drawn with CSS in the theme's colours, in every look and in high contrast, readable at 14 px), the word "Gemini" in the
usage bars, cards and tray notices, the same states as the others (no reading, billed, out). Text in every language by hand;
nothing new in layout, wording or order (the longest language at 720 px stays as it is).

Leave out: choosing Gemini in a picker for Chat (not offered), a setting, animation.

Done: a screen check in `tests/ui/` that renders a project reply, the running row, the usage bars and the tray text for a Gemini
conversation in all five languages and the three looks, with the mark's contrast measured (fails today: the type does not render);
a Rust test in `decide.rs` for the notice text. Extends: "Chat conversation" and the usage rows in `docs/product-map.md` and the
vendor mark paragraph of `docs/design/`. What's-new entry is carried by `gemini-profiles-in-routing`. Review: the other vendor and
the visual critique skill (before and after scores of the screens the diff touches, photos at 720 px). Cost: none.

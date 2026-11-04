# Show which CLI hooks Faber's agents will run and where they come from

Area: Settings
Files: engine/src/claude.rs, engine/src/engine/settings_change.rs, ui/Settings.tsx, ui/api.ts, ui/locales/, tests/ui/settings.check.ts, engine/tests/engine.rs, docs/product-map.md
Depends: none
Size: about 250 changed lines

Why: `docs/product/parity.md` row X6: both CLIs list their hooks (`/hooks`); Faber has one switch, "Your CLI hooks", and a person cannot tell what turning it on will run on their PC. A hook is a command, so seeing it first is a safety matter.

Build: a read-only list behind the existing "Your CLI hooks" row (its "?" fold, not a new row): for each hook the event, the command's first word and the file it comes from (user settings, the project's `.claude/settings.json`), read on demand and never kept. The engine reads only those settings files' `hooks` key, caps the list (for example 30), never prints a full command line (first word plus length) and never reads other keys. A file it cannot parse is named as unreadable. Words by hand in every language.

Leave out: editing hooks, Codex hooks if its files have no stable form (say so in the map), and running one.

Done: an engine test that fails today: a settings file with hooks gives the capped list and ignores every other key; a broken file is named; a screen check opens the fold and sees the lines. Extends the map row `userHooks`. Cost: one small file read when the fold opens.

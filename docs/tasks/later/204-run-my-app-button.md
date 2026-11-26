# Run my app with one button, and a clear notice when it does not start

Area: Page in Faber's browser
Files: engine-projects/src/project/checks.rs, engine/src/engine/, host/src/table.rs, ui/Site.tsx, ui/locales/, tests/ui/site.check.ts, docs/product-map.md
Depends: none
Size: about 600 changed lines

Why: plan item B4. Read on 2026-11-25: the app panel shows a page only when a command's output announced a local URL (`ui/Site.tsx` `appsOf`); `engine-core/src/map.rs` reads `package.json` scripts and `checks.rs` finds `test`, but nothing finds or starts the dev command.

Build:

- Find the project's dev command (package scripts and common files), confirm it once the way checks are confirmed, start it with no shell (AGENTS rule) and no window, read its URL, show it in the panel.
- A plain notice with the last lines when it exits or never prints a URL; stop it with the conversation.

Leave out: putting the project online (G8 is out of this update).

Done: tests with fake processes for found, started, failed and stopped; a screen check of the button and the notice. Processes and running project code: review by the other vendor (R146).

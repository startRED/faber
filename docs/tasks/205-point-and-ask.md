# Point at something in the running app and ask to change it

Area: Page in Faber's browser
Files: host/src/pane/, ui/Site.tsx, ui/Composer.tsx, ui/locales/, tests/ui/site.check.ts, docs/product-map.md
Depends: run-my-app-button
Size: about 500 changed lines

Why: plan item B3: click a button or text of the running app and say what should change. Saved task 960 is about clickable reply blocks, not this.

Build:

- A pick mode in Faber's browser: hover outlines an element, a click puts a short description (text, role, selector, a crop) into the composer as context.
- It leaves the page untouched; Esc ends the mode.

Leave out: editing the page directly.

Done: a screen check that a pick fills the composer with the element's description and that Esc ends it. Screen only: ships on its proofs.

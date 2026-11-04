# Mark a remembered file path that no longer exists as stale

Area: Project memory
Files: engine/src/project/memory.rs, engine/src/project/rules.rs, engine/src/engine.rs, ui/ProjectMemory.tsx, ui/locales/
Depends: start-card-in-sessions
Size: about 200 changed lines

Why: slice 2 of `docs/superpowers/specs/2026-10-22-own-work-method.md`, "Staying small and current". References are
checked only for backticked, project-relative file paths. At close, a path that no longer exists gets `(stale?)` and its
item leaves the card. Prose, commands and settings are never judged.

Build:

- At a conversation's close (the point where the closing box is written), scan the `.faber/` items that the card would
  load for backticked paths that look like project-relative files (no scheme, no `..`, not absolute) and test each with
  a no-follow metadata check inside the project folder, through the same handles the notes use. Bounded: at most 200
  paths per close.
- Write the ` (stale?)` mark into the item through `memory::edit` (a normal entry edit with its revision guard) and never
  touch an item that a person wrote with a matching record; for those, only hide from the card and show the mark in
  Memory. Remove the mark again when the path exists later.
- The card builder skips items that carry the mark; Memory shows the mark in plain words, in every language.
- No timer and no watcher: the check runs only at close.

Leave out: judging commands, URLs, settings names or prose.

Done: engine tests that fail today show a missing path marked, an existing one untouched, an escaping path (`../x`,
a link) never opened, and a marked item absent from the card; a screen check for the mark. Extends: the product-map rows
"Project memory" and "Project instruction and learning files". What's-new entry in every language. The commit body
states time per close. One review by the other vendor (R146).

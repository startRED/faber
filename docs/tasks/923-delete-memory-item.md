# Delete any stored item in Memory with one click, and its record with it

Area: Project memory
Files: engine/src/project/memory.rs, engine/src/store.rs, engine/src/engine.rs, ui/ProjectMemory.tsx, ui/locales/, docs/design/
Depends: taste-lesson-records
Size: about 250 changed lines

Why: slice 3 of `docs/superpowers/specs/2026-10-22-own-work-method.md`, "Staying small and current": one click in Memory
deletes any stored item and its record goes with it; `rules.md` is built, so Memory shows it for reading only. Today
`memory::edit` already deletes an entry (`Edit.text` of `None`, with a revision guard), but the hash left in
`decisions-kept.json` (and, after task taste-lesson-records, in the taste and lesson record) stays behind.

Build:

- Deleting an entry of `decisions.md`, `taste.md`, `lessons.md` or `next.md` also removes its record, in the same
  operation, so a later item with the same words is not silently approved again.
- One click in `ui/ProjectMemory.tsx` (the existing entry row, using the existing delete if the row has one) with an
  undo toast kept only in memory; `rules.md` rows show no delete control.
- Adopted sources (task adoption-in-the-card) are never deleted from here.

Leave out: bulk delete and any new surface.

Done: engine tests that fail today delete an approved item and find no record left, and refuse a delete on `rules.md`;
a screen check shows the control present on a stored item and absent on `rules.md`. Extends: the product-map rows
"Project memory" and "`decisions-kept.json`"; the design page for Memory. What's-new entry in every language. One review
by the other vendor (R146).

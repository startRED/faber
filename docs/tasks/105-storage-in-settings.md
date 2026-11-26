# Settings shows how much disk Faber uses, by kind and by project, with the safe clean-ups one click away

Area: Settings screen
Files: engine-store/src/storage.rs, engine/src/engine/, host/src/table.rs, ui/api.ts, ui/Settings.tsx, ui/StorageUse.tsx, ui/locales/, tests/ui/settings.check.ts, docs/design/settings.md, docs/product-map.md
Depends: none
Size: about 700 changed lines

Why: plan item C1. The owner's data folder is 8.2 GB on a C: drive with 24 GB free (copies 3.1 GB, reports 1.6 GB,
tools 1.5 GB, rewind 0.8 GB, helper transcripts 0.6 GB), and nothing on screen says so. Read on 2026-11-25: no Settings
section shows disk use; the five sections are General, Projects, Appearance, Accounts and limits, and Privacy
(`docs/product-rules.md`, settings live only there); no engine function sums a folder (`testkit/src/sweep.rs` has one for
development only).

Build:

- An engine measurement (`engine-store/src/storage.rs` or where the store lives): sizes by kind (copies, reports, tools,
  rewind, helper transcripts, conversations, the rest) and copies by project, never following links, bounded in entries
  and time, at background priority, run only when the screen asks (no background timer). Errors give "unknown", not 0.
- A host command in the table (free: reading) and the API call.
- A view inside an existing section (General or Projects; pick one and say why in the design page) with the total, the
  free space of that drive, the list by kind, and buttons only for clean-ups Faber already does safely (the copy sweep,
  old reports past their limit); every removal stays the person's click.
- Every text in `ui/locales/` in each language, written by hand (en is the source).

Leave out: choosing the data folder (C2), lighter copies (C3), removing anything Faber does not already remove.

Done: a unit test of the measurement on a temp folder with a link and an unreadable entry (fails without the code); a
screen check in `tests/ui/settings.check.ts` that the view lists the kinds and that the clean-up asks nothing extra.
Memory and processor cost of one measurement in the commit body. Extends `docs/design/settings.md` and the product-map
rows `Settings` and `Every setting` where it applies. Walked as a newcomer and a heavy user against the taste cards.

# Taste, lessons and follow-ups (own work method, slice 3)

If `docs/tasks/992-faber-folder-caps.md` is still on `origin/phase-1-base`, slice 2 has not landed: run
`node scripts/tasks.mjs release taste-lessons-next` and take another task.

Build slice 3 of `docs/superpowers/specs/2026-10-22-own-work-method.md` (approved as of commit 4fc1324d, R435).

- **Taste and lessons are proposals.** The agent proposes a taste line when the person corrects the same kind of thing
  a second time, or once with "always" or "never". It proposes a lesson at close. Both show on the closing box.
- **Only the engine writes them.** It writes `taste.md` or `lessons.md` after the person's yes, with a provenance
  record. The agent never writes them directly, and a line without a matching record is an unreviewed proposal.
- **Follow-ups.** `next.md` takes follow-ups at close, one line each, at most 200 characters with why, and 12 lines at
  most. The done card offers them, and a line goes when its work lands.
- **Deleting.** One click in Memory deletes any stored item, and its record goes with it. `rules.md` is read-only there.

Done means AGENTS.md "Done means":

- the product map;
- the design pages for the closing box, the done card and Memory;
- a what's-new entry in every language;
- a screen check that fails without the change;
- the measured cost.

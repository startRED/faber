# Each view of the right column opens at its own best width

The owner (2026-10-22): each pane, when it opens or closes, picks the best size for the screen. Today Running now,
Quick question, Changes, Faber's browser, visual answers, a file and Technical details share one stored width
(`faber.layout.run`, `autoRun` 360 to 560 px), so the browser opens as narrow as the running list. Design: section 1 of
`docs/superpowers/specs/2026-10-22-panes-tabs-and-space.md` (read it, and `docs/design/layout.md` on widths and edges).

1. Give each column view a kind and a usual width: browser, Changes, visual answer and file at 60% of the work area;
   Quick question 420 px; the running list, Commands, Technical details and helpers keep `autoRun`. The stage keeps
   440 px. A drag is stored per kind in `faber.layout.panes`; the old `run` value migrates to every kind once; a double
   click or Enter on the grip returns that kind to its usual width.
2. Switching views changes the width in the column's existing motion (instant with reduced motion); closing gives the
   conversation its room back.
3. Dock by room instead of the fixed 1,100 px: the column docks whenever the stage keeps 440 px beside it. When a wide
   view opens and the stage would drop under 560 px, the sidebar folds away with motion and comes back when the view
   closes; that fold is not stored.
4. Update `docs/design/layout.md`, the product map's `faber.layout` row, the `ui/layout.ts` tests and a screen check that
   opens the browser, then the running list, and measures both widths (it fails today).

Done when: the browser opens at about 60% and the running list at its own width in a 1,920 px window, a drag on one kind
leaves the others alone, it survives a restart, and the 720 px and 1,100 px layouts still pass their checks. Report the
memory and processor cost (expected none: CSS variables and one stored object).

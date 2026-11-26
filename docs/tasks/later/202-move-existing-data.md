# Move Faber's existing data to the chosen folder, safely

Area: Settings screen
Files: host/src/lib.rs, electron/main.ts, engine-store/src/, ui/Settings.tsx, ui/locales/, docs/design/settings.md, docs/product-map.md
Depends: choose-where-new-data-goes
Size: about 600 changed lines

Why: plan item C2, second part: moving live data needs Faber closed, a copy, a check and only then the switch.

Build:

- A move that runs with the engine stopped: copy, verify sizes and hashes, switch the stored choice, and leave the old folder until the person removes it from the storage view.
- Recovery after an interrupted move (the old folder stays the truth until the switch).

Leave out: removing the old folder automatically (that is the person's click).

Done: tests of a move interrupted at each step and of a full move; a screen check. Stored data: two reviews from different vendors (R146).

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

State (2026-11-27): a third attempt is on the branch `night/move-existing-data` (moves catalog tools one at a time while
Faber runs, with progress, Stop and a separate removal of verified originals). Its reviews (Codex, Antigravity) still
find: removal hashes and deletes under the process-wide tools lock; a helper's kit is not counted as "in use", so its
audio runtime can be deleted; Change or Reset that lands while the worker waits for the lock still lets one item publish
to the old folder; a retry after a crash can need room for two copies; the final source check can miss a same-size edit
whose time is restored; a folder replaced by a junction between verification and publication; the Unix location switch
is not proven durable before cleanup; and a destination that became part of a project later is not checked again.

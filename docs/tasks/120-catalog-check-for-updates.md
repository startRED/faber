# Catalog: check for updates, then update the chosen items or all at once

Area: Catalog screen
Files: ui/Catalog.tsx (or the catalog screen's files), ui/locales/, engine-projects/src/toolbox.rs, engine/src/engine/toolbox.rs, host/src/lib.rs, engine/tests/toolbox.rs, tests/ui/, docs/design/catalog.md, docs/product-map.md
Depends: none
Size: about 700 changed lines

Why: the owner asked on 2026-11-28: "the Catalog should let me update the skills, plugins or anything I downloaded
there: click to check for updates, see which have one, and update the ones I select, several, or all at once".
Today a moved pin shows an update-available line on each card, detail and Installed row, and each item is updated
alone through the agent or its review card (`docs/design/catalog.md`, "A moved catalog pin...", "Update and repair").

Build:

- An Updates place in the Catalog: a Check for updates button, the list of installed items whose checked pin moved
  (name, what changes in one line), a box per row, Select all, and Update selected / Update all.
- Check for updates compares every installed item (skills, plugins, servers, runtimes, audio models) with the
  current checked pins, and asks the app updater once whether a newer Faber is out; when one is, it says new tool
  versions come with it and offers the existing update button. Only checked pins are offered: never an upstream
  commit Faber did not check (R33, the seal).
- One confirmation for the batch, listing every item and the same facts the single update card states (local edits
  and an earlier backup are replaced, one backup kept, on/off kept, running conversations keep their kits).
- The batch runs the existing replacement journal one item at a time, with progress and Stop; a failed item is
  listed with its retry and never undoes the others. Disk room for the batch is checked before it starts.

Leave out: automatic updates; directory (open world) items; changing how a single item updates.

Done: engine tests of a batch with one failure, Stop between items, and a crash mid-batch (journal recovery); a
screen check of check, select, update all and the failed row; texts in all five locales by hand. Stored data:
review by the other vendor (R146).

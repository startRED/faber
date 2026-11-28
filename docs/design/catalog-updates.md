# Catalog updates

How an installed catalog item is updated or repaired: the moved pin, the check, group updates and the repair card.
Browsing, installing and Undo are in [Catalog](catalog.md).

A moved catalog pin keeps the installed copy, its on/off choice and its previous recovery copy, including while removing
or undoing another item. Its card, detail and Installed row show a localized update-available line, even when off or
the new pin has identical bytes. Review replacement is the existing manual update path; nothing downloads automatically.
New sessions omit bytes that fail the current digest check; healthy tools and existing conversation kits remain available.

Check for updates, beside the filters, reads the installed copies against the checked pins once and asks the app updater
once. Its line says how many updates there are, or that everything is up to date; when a newer Faber is ready it adds that
newer checked versions come with it and points to the sidebar's update button. Only pins Faber checked are offered, never
an item's newer upstream commit (R33). Moved pins open an Updates filter, shown only while something waits, with one
Update all button; the selection bar offers Update beside the other group actions. One question covers the whole group
and states what the single card states. Each item then gets a fresh plan and its own journal, one after another, so a
failure or a crash never undoes the items already done; failed items stay selected with Retry, and Stop lets the item in
hand finish and keeps the rest selected.

An installed item with a failed source digest check stays listed with one localized reinstall line on its card, detail
and installed catalog row. It is unavailable to new sessions; healthy items remain available. The repair shortcut opens this same detail. Review replacement captures the exact old copy before showing old and curated pins, the preserved on/off choice, and one retained backup. Confirm revalidates that snapshot and replaces through the journal; a later confirmed replacement supersedes that backup. Cancel, Back and Escape discard the question. Failure requires a new review. Batch installs accept only new copies; initial installation refuses occupied paths and Undo even without a ledger entry, and preserves off switches and unrelated ledger entries when current files are absent. Install errors keep their retry; transient snapshot I/O
errors do not mark healthy source files as needing reinstall. Catalog returns its current cached status without waiting
for snapshot preparation. A `catalog` publication event refreshes mounted cards and installed catalog rows without polling;
the listener closes with the tab, including when registration completes after closure. These controls and status never
return to Settings.

Update and repair ask on a consent card of their own, as removal does, with separate grants and show both curated pins. The card states that local edits and
an earlier backup are replaced; the current files become the one retained backup outside all launch roots. It keeps
the tool's on/off state, and running conversations keep their leased kits. An incomplete journal is recovered before
new catalog preparation, while unknown or changed recovery files remain preserved and block new delivery.

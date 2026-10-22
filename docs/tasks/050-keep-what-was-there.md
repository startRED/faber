# Faber keeps what the person left on screen

The same kind of fix as the browser's memory (`docs/superpowers/specs/2026-10-22-panes-tabs-and-space.md`, section 5),
from a sweep of the screen on 2026-10-22. Check each against the code first and skip one already fixed.

1. Each conversation's scroll place (an in-memory map; a search reveal still wins). Today every return jumps to the end
   (`ui/App.tsx`, `stick = true` on open).
2. The sidebar's open projects, stored with `faber.layout` (`ui/Sidebar.tsx` starts with only the current one).
3. The Catalog's filter, search, page and selection while Faber runs, and the Catalog as a place `faber.place` restores
   (`ui/Catalog.tsx` holds them as local signals).
4. Drafts in standing goals and helper replies through `keep` and `kept` (`ui/ProjectGoals.tsx`, `ui/AgentChat.tsx`).
5. The open column view per conversation, reopened on return and after restart (the browser through task 020).
6. Settings opens on its last section.

Each comes with a screen or unit check that fails without it, the product map's stored-value rows, and what's-new
entries.

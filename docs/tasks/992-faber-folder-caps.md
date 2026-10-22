# Soft targets, folding and stale paths in `.faber/` (own work method, slice 2)

If `docs/tasks/991-rules-and-start-card.md` is still on `origin/phase-1-base`, slice 1 has not landed: run
`node scripts/tasks.mjs release faber-folder-caps` and take another task.

Build slice 2 of `docs/superpowers/specs/2026-10-22-own-work-method.md` (approved as of commit 4fc1324d, R435). See
its "What lives in `.faber/`" table and "Staying small and current".

- **Soft targets.** Check each file's target. A file over its target shows "over its limit" in Memory.
- **Folding.** The next `wrap_up` asks the agent to fold an over-target file. When folding cannot reach the target
  without losing a live item, nothing is dropped.
- **`decisions-old.md`.** Past 8 KB, decisions that a later one overrides move there. It is searched and never loaded.
- **Stale paths.** At close, a backticked project-relative file path that no longer exists gets `(stale?)` and leaves
  the card. Prose, commands and settings are never judged.
- **Legacy names.** `plans`, `recipes`, `learning` and `memory` stay readable.
- **No timer and no watcher.**

Done means AGENTS.md "Done means":

- engine tests that fail without each rule;
- the product map and the design page for Memory;
- a what's-new entry in every language;
- the measured cost.

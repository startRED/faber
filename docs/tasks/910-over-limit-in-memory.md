# Mark a Faber note file that is over its target in Memory

Area: Project memory
Files: engine/src/project/memory.rs, engine/src/engine.rs, ui/ProjectMemory.tsx, ui/api.ts, ui/locales/, docs/design/
Depends: none
Size: about 250 changed lines

Why: slice 2 of `docs/superpowers/specs/2026-10-22-own-work-method.md` (section "Staying small and current"; approved
as of commit 4fc1324d, R435; a design change goes to the owner first). Each `.faber/` file has a soft target for what is
kept: `rules.md` 3 KB, `decisions.md` 8 KB, `taste.md` 1 KB, `lessons.md` 2 KB, `next.md` 12 lines. The card's shares are
hard limits on what is loaded; the file targets only mark. Today `memory::list_faber` lists those files with their
entries (`MemoryFile`) and nothing says when one is too big.

Build:

- A function with the five targets in one table, and a flag on `MemoryFile` ("over its limit", the size and the
  target) computed in `list_faber`; no timer, no watcher, no new read (the size is known where the file is read).
- Memory (`ui/ProjectMemory.tsx`) shows one quiet "over its limit" mark and the sizes on such a file, in plain words,
  in every language. `rules.md` is built, so it shows for reading only and is never reported as something to fold.
- Files that do not exist yet (taste, lessons, next are built by later tasks) are simply absent.

Leave out: folding and `decisions-old.md` (task fold-at-close-and-old-decisions), and any byte cap on what the person
types by hand.

Done: an engine test that fails today lists a 9 KB `decisions.md` as over its limit and a 7 KB one as not, and a screen
check shows the mark; walk it at 720 px and in the longest language. Extends: the product-map rows "Project memory" and
"Project instruction and learning files", and the design page for Memory. What's-new entry in every language. The
commit body states the cost (none while idle). One review by the other vendor (R146).

# Ask the agent to fold an over-limit file at close, and keep old decisions out of the card

Area: Agent's plan and closing box
Files: engine/src/progress.rs, engine/src/engine/plan.rs, engine/src/project/memory.rs, engine/src/memory_search.rs, engine/tests/plan.rs
Depends: over-limit-in-memory
Size: about 300 changed lines

Why: slice 2 of `docs/superpowers/specs/2026-10-22-own-work-method.md`, "Staying small and current". A file over its
target is folded at the next close: the agent merges duplicates and drops what a later line overrides. When folding
cannot reach the target without losing a live item, nothing is dropped and the file stays over its target. Past 8 KB,
`decisions.md` items that a later one overrides move to `decisions-old.md`, which is searched and never loaded.

Build:

- When the project has a file over its target (from the flag of task over-limit-in-memory), the description of the
  `wrap_up` tool (`WRAP_UP_DESCRIPTION` in `engine/src/progress.rs`) gets one extra instruction for that call only, naming
  the file: fold it, and move overridden decisions to `.faber/decisions-old.md`. No other turn changes.
- The agent does the moving (Faber cannot judge which decision is overridden); the engine checks afterwards that no
  item with a matching person record in `decisions-kept.json` was lost and shows a loss as a plain notice, not an error.
- `memory_search` (`engine/src/memory_search.rs`) reads `decisions-old.md` like the other `.faber` notes, marked as
  old; the card never loads it (the card builder takes only `decisions.md`).
- Legacy names `plans`, `recipes`, `learning` and `memory` stay readable and searchable; Faber never writes them. Add
  the test that proves it if none exists.

Leave out: any timer or automatic rewriting by the engine, and stale-path marks (task stale-path-marks).

Done: engine tests that fail today show the fold instruction present only while a file is over its target, a kept
decision missing after a fold reported, `decisions-old.md` found by `memory_search` and absent from the card, and a
legacy file still listed. Extends: the product-map rows "Agent's plan and closing box" and "Faber's project memory
search". What's-new entry in every language. The commit body states the token cost of the added instruction. One review
by the other vendor (R146).

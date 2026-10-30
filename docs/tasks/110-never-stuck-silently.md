# Keep going stops on a loop and says why

Area: Keep going
Files: engine/src/engine/keep_going.rs, ui/locales/en.ts, ui/locales/pt-BR.ts, ui/locales/es.ts, ui/locales/fr.ts,
ui/locales/de.ts, docs/product-map.md
Depends: none
Size: about 200 changed lines

Why: `docs/research/2026-10-29-faber-diagnosis/README.md`, part 2: in the owner's saved conversations two answers
alternated 19 times each under keep-going, and 255 automatic turns came with no text or tools. Items 1, 2, 3 and 5 of
the first version of this task landed in #444 (silence notice, compaction deadline, helper silence, repeated failing
step). Item 4 is left.

Build, in `engine/src/engine/keep_going.rs`, with a check that fails before the fix:

- Cap idle rounds (no text, no tools) as well as working ones.
- Stop on the same non-empty answer repeated (normalized) twice in a row, or two answers alternating.
- Give the two-empty-answers stop its own reason instead of `reason.keepGoing.done`, which claims the work is done.

A full draft is on branch `keep-going-loops` (commit "wip: task 110 all"): take its `keep_going.rs` part and the
`reason.keepGoing.repeated|silent|idle` and `tray.stopped.*` lines, never the parts #444 already landed. Words in every
language by hand, plain: "A IA repetiu a mesma resposta. Parei para não gastar sua cota." / "Parei porque a IA não
respondeu duas vezes; não sei se o trabalho terminou."

Done: the checks above in the engine tests, the keep-going entry of the product map says when it stops, a what's-new
entry in every language, one review by the other vendor.

# The AI's priority sentence says "quality always; economy and speed go together"

Area: none
Files: engine-drivers/src/persona.rs, engine/tests/persona.rs, docs/product/README.md, docs/next/
Depends: none
Size: about 60 changed lines

Why: plan item A4 and the owner's answer 7 of 2026-11-25 (section 7 of the plan): the priority rule becomes "quality
always; economy and speed go together", replacing R10 ("when speed and economy conflict, economy wins"). Today
`engine-drivers/src/persona.rs` says "Quality first, then economy, then speed: when saving tokens and saving time
conflict, save tokens.", `engine/tests/persona.rs` checks that order, and `docs/product/README.md` (around line 23)
says "When speed and economy conflict, economy wins."

Build:

- The new sentence in the persona, short and in English, with the same position and cost (count its tokens before and
  after; it must not grow by more than a few words).
- The test checks the new order and that the old "save tokens" tie-break is gone.
- `docs/product/README.md` says the new rule; a decision row in `docs/next/<branch>.json` with `R-next`, citing the
  owner's answer of 2026-11-25 and saying it supersedes R10.

Leave out: Auto's routing table and any measurement (task bench-switch-each-part measures).

Done: `engine/tests/persona.rs` fails on the old sentence and passes on the new; `npm run docs` passes.

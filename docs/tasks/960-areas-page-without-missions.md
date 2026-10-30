# Make the areas page say what Faber does today, without missions

Area: none
Files: docs/product/areas.md, docs/product-map.md
Depends: none
Size: about 150 changed lines

Why: missions were retired (R215) and a project conversation the agent starts replaced them (R198, R160, R161); Keep going
(R199) is the "nobody needs to be there" mode that exists. `docs/product/areas.md` still describes missions as live:
"start a mission with several models in parallel" (Universal by design), "starts a mission by itself when one breaks"
(Standing goals; R422 starts an ordinary conversation in a separate copy), "while the agent is answering or a mission is
running" (Normal and autonomous modes), and guardrails 6 and 7 ("A mission never stops...", "Every mission has an
estimate..."). The toolbox table of the same page already marks each row built, partial or not built, and the product
map's "Not built yet" paragraph repeats the list. Read both before editing.

Build, docs only:

- Reword each sentence above to what is true today, citing the decision that replaced it. Where the true statement is
  already in the same page (the toolbox table, Keep going, the plan estimate of `engine/src/engine/plan.rs`), point to it
  instead of repeating it.
- Where a sentence describes something not built and not retired (the Autonomous mode's switch "at any moment", the git
  modes Safe, Balanced and Free of R83), keep it and add "not built" with the word that says which part; do not rename a
  mode and do not invent what it allows. Those names and grants are the owner's.
- Keep the product map's "Not built yet" paragraph and the toolbox table consistent with the page after the edit.

Leave out: any code, any change of what a guardrail promises, and the owner's decisions on the modes.

Done: `npm run docs` passes and `engine/tests/product_map.rs` still passes; a search of `docs/product/areas.md` for
"mission" finds only the retirement notes. Extends: "Not built yet" of the product map. No memory or processor cost: docs
only. The owner reads the diff of a product page; send him one short card with the sentences that changed meaning.

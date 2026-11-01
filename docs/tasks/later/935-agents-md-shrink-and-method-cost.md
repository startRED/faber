# Let the engine's loop enforce "Done means", then shrink AGENTS.md and measure the method

Area: none
Files: AGENTS.md, docs/dev/workflow.md, engine/examples/bench.rs, docs/dev/benchmark.md, docs/superpowers/specs/2026-10-22-own-work-method.md
Depends: independent-review-gate, adapters-for-faber-own-files, start-card-token-benchmark
Size: about 250 changed lines

Why: slice 4 of `docs/superpowers/specs/2026-10-22-own-work-method.md`, "Faber's own development": once the engine's loop
(contract, checks, review gate, receipt) enforces what AGENTS.md "Done means" says by hand, AGENTS.md keeps only the
repo-specific lines (landing, lanes, what's-new) and points at the spec. The spec also asks that slice 4 reports the
whole method's added cost (reviews, folding and the other extra calls) on the same scenarios as the card benchmark, with
input, cached input and output apart.

Build:

- Walk each line of "Done means" and "Before a change a person will see or use" in `AGENTS.md`; for each, name the part
  of the loop that now enforces it, or keep the line. A line goes only when a check fails without the enforcing part.
  AGENTS.md rule: a rule a test enforces leaves it.
- Run the benchmark of task start-card-token-benchmark again with the whole method on, and write the added cost beside
  the card's in the spec's "Cost" and `docs/dev/benchmark.md`. If a gate of the spec fails, stop and ask the owner one
  short card.
- Keep the spec's note that the map step has not converged; do not move the product map.

Leave out: the product-map step (a later proposal, one step at a time with the owner's yes for each).

Done: `npm run docs` and the scripts' tests pass with the shorter AGENTS.md; the measured table is in the commit body.
Extends: AGENTS.md "Done means" and `docs/dev/workflow.md`. The owner reads the AGENTS.md diff before it lands (a card:
it is his rulebook). Review by the other vendor (R146).

# The bench switches each part of the method on and off and prices every run

Area: none
Files: engine/examples/bench.rs, docs/dev/benchmark.md, scripts/bench/
Depends: none
Size: about 300 changed lines

Why: slice 6 of `docs/superpowers/specs/2026-10-30-faber-method-2.md`, "Measuring it". A part of the method stays only
while the bench shows it helps.

Build:

- A switch per part: start card, packet, waves and join, review gate. The same request runs with the part on and off in
  paired runs (task 905's gate unchanged).
- Cost per accepted task at each model's API rates with cache, counting failed attempts, joins, repairs and reviews; a
  run whose cost is not fully known cannot pass. Time from the request to the accepted result.
- Every run records each vendor that contributed. A one-vendor run is judged blind by the other vendor with the
  executable proof; a mixed run counts only when the proof passes and both vendors, blind in fresh sessions, agree, and
  the report lists three such verdicts for the owner to check by eye.
- The plain CLI keeps its own subagents.

Done: unit tests that fail today for the pricing, the unknown-cost rule and the mixed-run rule; one dry run in the pull
request. Runs stay inside the US$25 test allowance. One review by the other vendor (R146).

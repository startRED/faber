# Measure what the start card costs in tokens before it stays

Area: none
Files: engine/examples/bench.rs, bench/requests/, docs/dev/benchmark.md, docs/superpowers/specs/2026-10-22-own-work-method.md
Depends: start-card-in-sessions
Size: about 350 changed lines

Why: the spec's "Cost" section (`docs/superpowers/specs/2026-10-22-own-work-method.md`) makes a token gate for the card,
declared before any run. Compare identical Faber configurations with and without the card on the five R410 requests
(`bench/requests/`, run by `cargo run -p faber-engine --example bench`, see `docs/dev/benchmark.md`), three paired runs
each, on both CLIs, in cold, warm and resumed sessions; report input, cached input, output and extra model calls apart,
with the spread. It passes only with no fewer successes and at most 5% more input-plus-output tokens with the card; a
difference inside the spread earns three more pairs, then counts as failing. A pass covers this suite only. If no card
passes, the owner chooses between a smaller card and none.

Build:

- A bench switch that starts Faber with and without the card (an engine setting read only by the bench, not a user
  setting), an alternating pair order, and the session conditions cold, warm and resumed.
- The report table and the gate verdict computed by a plain function with unit tests (the spread rule and the
  three-more-pairs rule included).
- Estimate the spend first (`--dry-run`). The owner authorized test spend under US$25 in total; if the full matrix
  would pass it, run the part that fits and ask the owner one short card for the rest (money is theirs).
- Record the measured result in the spec's "Cost" and in the product map's start-card row; if the gate fails, stop and
  ask the owner.

Leave out: any change of the card itself.

Done: the unit tests for the gate fail without the rule; the measured table is in the commit body and the dated result
in `docs/dev/benchmark.md`; the bench never runs in CI or `cargo t`. Extends: the benchmark page and the product-map
start-card row. Review by the other vendor (R146).

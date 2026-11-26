# Measure what each part Faber adds to a turn costs in time and tokens, and what it buys

Area: none
Files: engine/examples/bench.rs, bench/pure.rs, bench/, engine/src/engine/bench_switches.rs, docs/dev/benchmark.md, docs/dev/release-1.0.md
Depends: priority-quality-always
Size: about 450 changed lines

Why: plan item A4, first half ("measure each part on and off, cut only what does not change the judge's score"). On
2026-11-10 (`docs/dev/release-1.0.md`) the same request took Faber about 55% longer than the plain CLI (Codex 615 s
against 397 s, Claude 255 s against 166 s) and Codex 29% more tokens (1.24 M against 0.96 M), while the written goal is
at most 70% of the plain CLI's cost and a blind-judge win or tie in 9 of 10 (`docs/product/README.md`, Goals). Read on
2026-11-25: the bench (`cargo run -p faber-engine --example bench`, `docs/dev/benchmark.md`) has `--only`, `--plain`,
`--vendors`, caps, `--workflow-pair` and `--dry-run`, but no switch that turns one part of Faber off. Saved task 048
does this for the method parts but depends on a chain (044, 043, 933, 932) still in `later/`; this task is the smaller
step the plan needs now.

Build:

- List, from the code, what Faber adds to a project turn compared with the plain CLI: its instructions (persona and
  guide), the start card, the self-check offer and checks (R565), review rounds, the router call of Auto, and any other
  call the code shows. One bench-only switch per part (an engine option the bench sets, never a user setting).
- The report gives, per part, time, input, cached input, output and extra model calls with the part on and off, in
  paired runs with alternating order, and the judge's verdict for both sides (blind, the other vendor).
- Estimate the spend with `--dry-run` first. Run once, the smallest set that gives a verdict per part: three requests,
  one vendor per side, one pair each. Anything bigger waits for the owner (quota is his).

Leave out: cutting or changing any part (task cut-what-the-judge-does-not-miss); task 048's mixed-vendor rules.

Done: unit tests in `bench/pure.rs` for the switch plumbing and the paired report (they fail without the code); the one
measured table in the commit body and, dated, in `docs/dev/benchmark.md`; the bench never runs in CI or `cargo t`.
Extends the product-map row for the light benchmark. Engine options read only by the bench: ships on its proofs.

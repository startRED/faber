# Turn off by default the parts whose cost the judge does not miss

Area: none
Files: engine/src/engine/bench_switches.rs, engine-drivers/src/persona.rs, docs/dev/benchmark.md, docs/product/README.md, docs/next/
Depends: bench-switch-each-part
Size: about 300 changed lines

Why: plan item A4, second half, and the owner's rule of 2026-11-25 ("quality always; economy and speed go together").
The measurement task gives, for each part Faber adds to a turn, its time and tokens and the blind judge's verdict with
it on and off.

Build:

- For each part whose removal cost no judge verdict and saved time or tokens beyond the spread, make it lighter or off
  by default; keep every part that won a verdict. A part that changes what a person sees or sets is the owner's
  decision: write the question in the commit and leave it on.
- Confirm the saving with cheap requests only (for example `5-everyday-question`, `2-plan-an-app`, `1-organize-folder`,
  `6-fix-invoice`), each pair run three times, comparing medians and the spread. The owner said on 2026-11-28: no big
  bench (the ~US$ 148.50 table is out); the whole confirmation stays under US$ 10 of API-price equivalents (`--max-usd`).
- Update the numbers in `docs/dev/benchmark.md` and the goals line in `docs/product/README.md` if they moved.

Leave out: parts the measurement did not cover.

Done: the confirming table in the commit body (before and after, median and spread of three runs, time and tokens, verdicts); the tests that covered
each part still pass or say why they changed. Engine behaviour: review by a fresh session of the other vendor (R146).

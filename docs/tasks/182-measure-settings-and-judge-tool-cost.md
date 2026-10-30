# Measure what the settings and judge tools add to every project request

Area: none
Files: engine/examples/bench.rs, engine/src/toolbox.rs, engine/src/creative.rs, docs/product/areas.md, docs/product-map.md
Depends: none
Size: about 150 changed lines

Why: `docs/product/areas.md` ("Toolbox") says the `settings` tool's added prompt cost "has not been measured", and that the
`judge_creative` definition, offered with the project toolbox, "spends quota only when called; its prompt-token overhead
has not been measured". Faber's own rule (AGENTS.md) is that every change reports its cost, and the figures on that page
for the earlier tools (about 400 tokens per project-agent request for the first three, about 350 more for
`offer_bigger`) show it is done by measuring. Each project request pays for every tool definition offered, so the
number matters (tokens are the first goal of `docs/product/README.md`).

Build:

- Find how the earlier figures were taken (the product map's notes on R412 and the commit that added them) and repeat the
  method, so the numbers compare.
- Measure, on both CLIs, a minimal fixed request with the toolbox offered with and without `settings` (the definition in
  `engine/src/toolbox.rs`) and with and without `judge_creative` (`engine/src/creative.rs`): input tokens as the CLI
  reports them, cold and cached, three samples each. Use the cheapest model; the owner's test spend is under US$25 in
  total and this is a few cents.
- Write the results where the "not measured" statements are, in `docs/product/areas.md` and the product map, with the
  date and the CLI versions. If either definition costs more than about 150 tokens per request, shorten its description
  text in the same change and measure again.

Leave out: any other tool, and any change of what a tool does.

Done: the numbers are in the pages, the "has not been measured" sentences are gone, and a unit test fixes the length of
each definition so a later growth shows (a failing check today does not exist: add it). Extends: the toolbox rows of the
product map. No memory or processor cost beyond the measured tokens: say so in the commit body. Review by the other
vendor (R146).

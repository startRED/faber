# A real-CLI check of when the project agent splits a request

Area: none
Files: engine/examples/leader_eval.rs, docs/dev/testing.md
Depends: none
Size: about 300 changed lines

Why: the owner's request is that a big request makes the project agent lead by itself and a small one stays direct (R160,
R161: "it decides alone when to split a request"). Today nothing measures that decision. Faber builds the agent's tools and
its instructions in `engine/src/claude.rs` (`--append-system-prompt`) and `engine/src/codex.rs` (developer instructions) from
`engine/src/persona.rs` (`persona`, `PROJECT_DETAILS`, `BIGGER_OFFER`); the conversation tool that starts team conversations
is `engine/src/sibling.rs` (`start`, at most `MAX_RUNNING` = 3 per project). `engine/examples/route_eval.rs` and
`engine/examples/cache_probe.rs` are the pattern for a manual example that runs the real CLIs on the owner's subscriptions
(tests and comparisons under US$25 in total need no question).

Build `engine/examples/leader_eval.rs`: 12 hand-labelled requests in English (6 big: "add dark mode to every screen, with a
settings switch, tests and docs", "migrate the three services from X to Y"; 6 small: "rename this function", "why does this
test fail"), each run on a fresh copy of a small fixture repository in a scratch folder under `%TEMP%` (never inside the
repo, never a build there), through the real `Engine` with a project conversation so the persona and the Faber tool server
are exactly the product's. The example sums, per request and per vendor, how many `conversation` `start` calls and how many
`plan` steps the agent made, and prints a table: big requests that started at least 2 conversations, big requests that made a
plan of at least 3 steps, small requests that started none (the number that must stay at 6 of 6), total tokens and API-equivalent
dollars. It stops a run as soon as the agent has started its conversations (no need to let them work), so a run is one short
turn. Print the labels and the verdict per request so a disagreement with a label can be read, not only counted.

Leave out: running the started conversations to completion, judging the quality of the split (the next task only needs the
decision), a CI job (this is a manual measurement like `route_eval`).

Done: the example builds and runs (`cargo run -p faber-engine --example leader_eval`), its table for today's instructions is
pasted into the pull request body as the baseline (expected: few or no big requests started 2 conversations), and a unit test
in the example's module proves the counting over a recorded event list (the `conversation` and `plan` calls of
`tests/fixtures/claude/` or a hand-written list). `docs/dev/testing.md` gets one paragraph that names the example and when to
run it (before and after a change to the agent's instructions or tools). New: no product-map entry (a developer measurement);
no what's-new entry. Cost: states the dollars of the baseline run. One review by the other vendor.

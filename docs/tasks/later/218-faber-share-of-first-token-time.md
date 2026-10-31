# Measure and cap the time Faber adds before the first word of a reply

Area: none
Files: engine/tests/first_word_cost.rs, engine/Cargo.toml, docs/product/README.md, docs/dev/benchmark.md, docs/product-map.md
Depends: none
Size: about 200 changed lines

Why: one of Faber's goals is "time to first token with a warm session, ≤ 2 s" (`docs/product/README.md`, the goals
table), manual fake-CLI first-word measurements exist (ignored tests in `engine/src/engine/kits.rs`, results in
`docs/product-map.md`), but no automatic budget guards the public send-to-first-text path, and `docs/dev/benchmark.md`
says speed to first token is "measured elsewhere or not yet". The model's own latency needs a live
account, but the engine's share (from the public engine send to the first reply-text event: routing, kit, prompt
building, the pipe, parsing, saving; the screen's own handling and rendering stay outside) can be measured on every commit with the fake CLI, and it is the only part Faber's code can make worse.

Build:
- A test program `engine/tests/first_word_cost.rs` (its own process, like `engine/tests/project_facts_cost.rs`): an
  engine with a warm session (the conversation already answered one turn, the fake CLI still running), per vendor, sends
  a message and times from `send` to the first `AppEvent` carrying the reply's first text, with the fake CLI answering at
  once. Reuse the fake backend and setup of the ignored first-word tests in `engine/src/engine/kits.rs`. Repeat it (at
  least 20 sends), print the fastest and the median, and enforce a cap on the fastest sample only. Also time the cold
  case (first message of a new conversation) and print it without a cap.
- Cap: calibrated from measured numbers with wide room but below the 200 ms injected regression (state the measured
  fastest, median and the chosen cap in the test's comment).
- `docs/product/README.md` and `docs/dev/benchmark.md`: say that Faber's own share is capped by this test and that the
  model's share is measured only by hand with a live account.

Leave out: the model's latency, Auto's routing call when it waits for a model (measure it separately only if Auto is off
in the warm case, and say so), any optimisation (a follow-up task if the numbers show one).

Done: the test passes on the PC and in CI's rust suite, and fails when a 200 ms sleep is added before the CLI write
(show that run in the pull request). Extends: "Features and their checks" in `docs/product-map.md` (name the check).
What's-new: none. Review: a fresh read-only session checks that the timing points are the real send and first text.
Cost: one test process of a few seconds in CI; none in the app.

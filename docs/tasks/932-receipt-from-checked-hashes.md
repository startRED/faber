# Write the receipt of a task from recorded results tied to file hashes

Area: Agent's plan and closing box
Files: engine/src/verify/journal.rs, engine/src/verify.rs, engine/src/engine/plan.rs, engine/src/model.rs, engine/tests/verify.rs
Depends: contract-in-the-plan
Size: about 350 changed lines

Why: slice 4 of `docs/superpowers/specs/2026-10-22-own-work-method.md`. The engine writes a task's receipt from
recorded results, each tied to the hashes of the files it saw; a later change voids the result. A receipt is what the
closing box may call "done" (guardrail 4 of `docs/product/areas.md`). Today the results journal
(`engine/src/verify/journal.rs`: project, command, input key, ok, time) orders runs and spots flaky checks, but nothing
ties a result to the files a task changed, and the agent's own words decide "done".

Build:

- A receipt built by the engine, never by the agent, at close: for each check of the plan's contract (task
  contract-in-the-plan) the latest journal entry for the conversation's folder, whether its input key still matches the
  files now (`verify::input_key`), and so whether the result still stands or is void.
- Stored with the conversation record (small: command, ok, key, time) and shown to the closing box as two facts: "N
  checks passed on these files" or "a check no longer matches your files".
- A plan with no contract gets the receipt of the checks that ran, with no claim of covering the plan.

Leave out: the review gate (task independent-review-gate), blocking "done" (this task only records and shows), and any
new check runner.

Done: engine tests in `engine/tests/verify.rs` that fail today show a receipt standing after a pass, void after a file
changes, and absent for a plan with no check; the commit body states the memory and processor cost. Extends: the
product-map rows "Agent's plan and closing box" and "Faber's checks". What's-new entry in every language. One review by
the other vendor (R146).

# Ready plan steps that do not overlap start together, each from the leader's current commit

Area: Conversation started by the agent
Files: engine/src/engine/plan.rs, engine/src/engine/siblings.rs, engine/src/engine/copies.rs, engine/src/engine/helpers.rs, engine/src/persona.rs, engine/tests/, docs/product-map.md, docs/design/
Depends: plan-steps-files-and-waits
Size: about 400 changed lines

Why: slice 2 of `docs/superpowers/specs/2026-10-30-faber-method-2.md`, "The loop of one request", step 3. A plain CLI does
one thing at a time; Faber's gain is running independent steps at once. Today the leader starts team conversations by
its own prose (`persona::LEADS`), each copy from the person's checked-out head (R201), so a later step cannot start from
the leader's latest work.

Build:

- A wave: the ready steps (task 041's `ready`) whose declared `files` do not overlap (a folder covers what is under it).
  A step with no `files` runs alone. How many start at once follows the existing working cap (3) and quota room.
- A writing step starts as a team conversation whose copy is made from the leader's current commit, as a branched
  conversation's copy starts from its source (R201); the copy's branch and base commit are recorded on the step.
- Read-only steps (search, collect, review) go to a helper instead and get no copy.
- The step's state follows its conversation: running, then waiting for the join.

Leave out: the join and the check of real changes (task 043), the packet (task 044); the worker gets today's brief.

Done: engine tests with the fake CLI that fail today: two non-overlapping ready steps start two conversations at once,
each copy at the leader's commit; overlapping steps start one after the other; a step waiting on another does not start.
Extends: "Conversation started by the agent". The plan's design page updated. What's-new entry in every language (a
person sees steps run side by side). One review by the other vendor (R146).

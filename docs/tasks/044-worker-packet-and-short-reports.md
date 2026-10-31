# Each worker gets a packet instead of a history and reports back in a short summary

Area: Conversation started by the agent
Files: engine/src/engine/siblings.rs, engine/src/engine/helpers.rs, engine/src/helper.rs, engine/src/persona.rs, engine/src/engine/kits.rs, engine/tests/, docs/product-map.md
Depends: plan-steps-files-and-waits
Size: about 350 changed lines

Why: slice 3 of `docs/superpowers/specs/2026-10-30-faber-method-2.md`, "The packet". A worker should start with what it
needs and nothing else, and the leader's context should grow by a few lines per step, not by every worker's transcript.

Build:

- The engine builds the packet for a step: the person's request and later corrections word for word (past 2 KB, the part
  this step serves plus a pointer to the whole), the step (goal, files, check, contract; 1 KB), the start card (6 KB,
  built) and pointers (0.5 KB). Byte limits checked by code.
- A worker reads the whole request, the corrections and the attachments through a Faber tool in bounded pieces, so
  nothing the person said is out of reach.
- A worker's report is a summary of at most 1 KB that always says whether anything failed and names, by full path in the
  worker's copy, the file that holds the failure lines, logs and long output whole.

Leave out: summarising tool output inside a CLI session (the CLIs run unmodified).

Done: engine tests that fail today: a packet never exceeds its limits and keeps the request word for word when it fits;
a long request carries the pointer; a report over 1 KB is refused with its key; a failed report always names its file.
A measured note in the pull request: the leader's input tokens on one five-step request, before and after. One review
by the other vendor (R146).

# A project shows one derived line of progress, and each plan step its state

Area: Agent's plan and closing box
Files: engine/src/progress.rs, engine/src/engine/plan.rs, ui/Plan.tsx, ui/Projects.tsx, ui/running.ts, ui/locales/, tests/ui/, docs/product-map.md, docs/design/
Depends: plan-steps-files-and-waits
Size: about 300 changed lines

Why: slice 1 of `docs/superpowers/specs/2026-10-30-faber-method-2.md`, "What a person sees". A person who does not
program should see where the work is without opening every conversation, and nobody should keep that status by hand.

Build:

- One line on the project, always derived from its plans, its check results and the cards waiting for the person:
  "3 of 5 steps checked, 2 running, 1 needs you" (each part only when it is not zero).
- Each plan step shows one of: waiting, running, checking, done, needs you, in plain words (no branch, merge or CI talk).
- No stored status: the line and the states are computed when shown.

Leave out: the GitHub board for a person's project (task 047); opening a step's conversation is task 040.

Done: a screen check that fails today shows the line with a running and a waiting step and updates when one is checked;
a 720 px window and the longest language fit. Extends: "Agent's plan and closing box". What's-new entry in every
language. Coherence review and visual critique (R172). One review by the other vendor (R146).

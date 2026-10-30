# Make the agent state its contract in the plan before it writes

Area: Agent's plan and closing box
Files: engine/src/progress.rs, engine/src/model.rs, engine/src/engine/plan.rs, ui/Plan.tsx, ui/locales/, docs/design/
Depends: none
Size: about 350 changed lines

Why: slice 4 of `docs/superpowers/specs/2026-10-22-own-work-method.md` ("The loop, sized by risk"; approved as of commit
4fc1324d, R435). The contract goes in the plan: `Extends:` or `New:` (the closest existing thing and why it does not
fit), the proving check, and a target when the work is about speed, size or cost. Today the `plan` tool
(`plan_schema` in `engine/src/progress.rs`) carries steps and an estimate only (`Plan`, `PlanStep`, `Estimate` in
`engine/src/model.rs`), and `verify` only runs checks afterwards (`docs/product/areas.md`, the `proof.define` row).

Build:

- Optional fields of the plan input and of `Plan`: `contract` with `extends` or `new_because`, `proof` (a check the
  project declares or a plain sentence) and `target` (a plain sentence, only for speed, size or cost work); all bounded
  where the input is parsed.
- A sentence in the `plan` tool description saying when a contract is needed (a change a person sees or a change of
  behaviour), and none for a question or a small fix.
- Plan card (`ui/Plan.tsx`) shows the contract as two short lines, in the person's language, inside the existing plan;
  nothing new to click. A plan with no contract shows exactly what it shows today.
- The contract is stored with the plan (it is replaced whole with the plan, like the steps).

Leave out: size (task size-raised-by-permissions), the receipt (task receipt-from-checked-hashes) and any enforcement:
this task only records and shows what the agent promised.

Done: engine tests that fail today parse, bound and keep the contract and show a plan without one unchanged; a screen
check shows the two lines; the commit body states the token cost of the added description text. Extends: the
product-map row "Agent's plan and closing box" and the design page for the plan. What's-new entry in every language.
One review by the other vendor (R146), plus the coherence review of a change a person sees.

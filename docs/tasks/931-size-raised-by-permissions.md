# Size the work at plan time and raise it when a call crosses a boundary

Area: Agent's plan and closing box
Files: engine/src/progress.rs, engine/src/model.rs, engine/src/permissions.rs, engine/src/engine/plan.rs, engine/tests/plan.rs
Depends: contract-in-the-plan
Size: about 350 changed lines

Why: slice 4 of `docs/superpowers/specs/2026-10-22-own-work-method.md`, "The loop, sized by risk". The size (small, in one
area, across areas, or security) is set at plan time. The engine raises it before a call runs, from the permission layer,
when the call crosses a boundary or cannot be classified; the call waits for the contract. The size decides how much of
the loop applies later (the review gate and the receipt).

Build:

- A `size` on the plan (an enum of the spec's levels) set from the plan input, defaulting to the lowest.
- A pure function in `engine/src/permissions.rs` that classifies a tool call's boundary (touches the data folder or a
  credential path, leaves the project, publishes or deletes outside a copy, or cannot be classified because the shell
  syntax is unsupported) and returns the size it demands; the engine raises the plan's size to it before the call is
  allowed, and a call that demands more than the contract covers waits for the contract (a question card with the
  default of stating it), never fails silently.
- Nothing is lowered automatically; a person's Stop and the existing permission cards are unchanged.

Leave out: what each size triggers (tasks receipt-from-checked-hashes and independent-review-gate) and any screen beyond
one plain line saying the size was raised and why.

Done: engine tests that fail today show each boundary class raising the size, an unclassifiable command raising it, a
plain read leaving it, and the call waiting for the contract; the commit body states time per call (it runs on every
tool call, so measure it). Extends: the product-map row "Agent's plan and closing box". What's-new entry in every
language. Two reviews, one by the other vendor, because this is security (R146).

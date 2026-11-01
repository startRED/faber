# Require a fresh read-only review before a sized task counts as done

Area: Agent's plan and closing box
Files: engine/src/helper.rs, engine/src/engine/helpers.rs, engine/src/progress.rs, engine/src/model.rs, engine/src/route.rs, engine/tests/project_review.rs
Depends: size-raised-by-permissions, receipt-from-checked-hashes
Size: about 400 changed lines

Why: slice 4 of `docs/superpowers/specs/2026-10-22-own-work-method.md` and guardrail 5 of `docs/product/areas.md`: the
writer never reviews its own work; the reviewer is the other vendor when its quota allows, with the same-vendor
fallback of R146 and "not reviewed" when none can start. The areas page marks `review.request` as not built. Today the
person's Review buttons in Changes (R279) send a prompt through the conversation and the helper, and nothing makes a
review a condition of "done".

Build:

- A gate that, for a plan whose size asks for it (task size-raised-by-permissions), starts a fresh read-only session
  (the other vendor through the existing helper, or the same vendor in a new read-only session when its quota is poor)
  and gives it the contract, the receipt and the diff; its answer is recorded with the receipt.
- Blocking classes clear only by a fix or by an answer that the reviewer or the person accepts (R147: only a break for a
  person, data, security or a written rule blocks). Security takes two reviews, recorded separately.
- When no reviewer can start, the closing box says "not reviewed" in plain words; the task is not called verified.
- The gate uses quota only when a size asks for it; show the estimate before it runs if the plan has one.

Leave out: the receipt itself (task receipt-from-checked-hashes) and the Changes buttons, which stay as they are.

Done: engine tests in `engine/tests/project_review.rs` that fail today show the gate starting for a security-sized plan
twice, a blocking finding stopping "done" until answered, the same-vendor fallback and the "not reviewed" ending; the
commit body states the token and quota cost on a sample task. Extends: the product-map row "Conversation changes" and
"Agent's plan and closing box", and the toolbox table in `docs/product/areas.md` (`review.request`). What's-new entry in
every language. Two reviews, one by the other vendor, because this is security (R146).

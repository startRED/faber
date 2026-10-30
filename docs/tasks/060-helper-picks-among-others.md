# A helper or reviewer comes from whichever other AI has the most room

Area: Helper from the other AI
Files: engine/src/helper.rs, engine/src/engine/helpers.rs, engine/src/engine/recovery.rs, engine/src/engine/creative_judges.rs, engine/tests/, docs/product-map.md
Depends: gemini-vendor-variant
Size: about 220 changed lines

Why: the owner wants reviews by "the other vendor" and quota balanced over Claude, Codex and Gemini. The helper tool is built on
exactly two AIs: `helper::other(vendor)` (`engine/src/helper.rs`) returns the single other one, `offers_helper`
(`engine/src/engine/helpers.rs`) offers the tool when that one is connected, `recovery.rs` finds "the other" with
`Vendor::ALL.into_iter().find(|v| *v != t.pick.vendor)`, and the creative judges iterate `Vendor::ALL` for spare capacity. With
a third AI, "the other" is a choice: it must be a connected AI that did not write the work, with quota to spare (the gate of
`Core::allows` with `Purpose::UserTurn { by_hand: false }`, never billed per use, never the one the pace says is nearly spent).

Build: `helper::others(vendor, connected)` returns every connected AI except the writer; the helper runs on the one with the most
room by the same pace reading Auto uses (`budget.rs` `profile_m` / `vendor_m`; ties broken by the order Claude, Codex, Gemini);
`offers_helper` is true when any other is connected and allowed; recovery's "move to another AI" and the judges use the same
function, so there is one rule. The tool description and the helper row keep saying "the other AI" in the plural-neutral way
("an AI that did not write this"). With exactly two connected AIs every behaviour is unchanged.

Leave out: a new setting, letting the agent name the AI (the team `start` has `ai`; the helper keeps choosing by itself), any
screen change.

Done: tests that fail today: with Claude writing and both others connected, the helper goes to the one with more room and to the
other when the first is out; with two connected AIs the old tests (`helper.rs` unit tests, `engine/tests/` helper and recovery
tests) pass unchanged. Extends: "Helper from the other AI" and "Quota reading, pacing, recovery" in `docs/product-map.md`. No
what's-new entry (the result is the same words, a better choice). Review: the other vendor. Cost: none at rest.

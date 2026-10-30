# The agent chooses the AI of a team conversation and lets it go through a list by itself

Area: Conversation started by the agent
Files: engine/src/sibling.rs, engine/src/engine/siblings.rs, engine/src/engine/keep_going.rs, engine/tests/engine.rs, docs/product-map.md
Depends: none
Size: about 330 changed lines

Why: the owner wants the work spread "by quota" and a team that keeps moving without his message. Today
`Core::start_sibling` (`engine/src/engine/siblings.rs`) gives every started conversation the parent's own model choice
(`let choice = parent.choice.clone()`), so a parent pinned to one AI starts a whole team on that AI, and the gate it passes
(`sibling_allowed`) checks the parent's AI, not the new conversation's. The agent has no way to say "this one on the other AI"
even when its `quota` call shows the first AI nearly spent. And a team member that should go through a list of tasks ends its
turn after the first one, because nothing turns Keep going on for it (`Core::set_keep_going`, `engine/src/engine/keep_going.rs`,
only the person's chip does). Under Auto the child is already routed by quota pace at its first message (`engine/src/engine/auto.rs`,
a conversation then stays in its own subscription), so the gap is the pinned parent and the list.

Build: two optional fields of the `conversation` tool's `start` (`sibling.rs`: schema, `Request::Start`, `request()`, the long
description; the screen needs nothing):
- `ai`: `"same"` (default, today's behaviour), `"auto"` (Auto routes the brief by the table and the quota pace, whatever the
  parent is pinned to), or a vendor name of a connected AI (`"claude"`, `"codex"`). Build the child's `Choice` from it (a named
  vendor takes that vendor's default model at medium effort, `router::pick_model`), refuse a vendor that is not connected with
  a sentence the agent reads, and run the quota gate (`sibling_allowed`) for the child's vendor, so billed-per-use or out-of-quota
  AIs are refused exactly as for a helper (a named AI that cannot run says so and the agent chooses again; "auto" is gated when
  it is routed).
- `keep_going`: true turns Keep going on for the new conversation after its first turn is sent, with the same eligibility as
  the chip (subscription AIs only, no billed-per-use spending unattended); the brief should hold an ordered list of tasks, and
  the conversation goes through them. The child's report to its parent must come once when its Keep going ends, not at every
  turn (`tell_parent` already returns while `keep_going` is set; check that the end of Keep going calls it, add the call if not).
Tell the tool description when to use each (`ai`: spread work over the AIs that have room, a review on the AI that did not write
the code; `keep_going`: a list of tasks for one conversation).

Leave out: choosing a model or effort (the AI's default is right for a team member), any screen change (the answer header
already shows each answer's AI and model), a third AI (the Gemini tasks extend `ai` later).

Done: tests in `engine/tests/engine.rs` next to `the_agent_starts_a_conversation_of_its_own_and_reads_where_it_stands` that fail
today: a parent pinned to Claude starts a conversation with `ai: "codex"` and its first turn runs on Codex (fake CLI); `ai:
"auto"` reaches the router; an unconnected or out-of-quota vendor is refused with the reason; `keep_going` runs three turns of
a fake list and the parent receives one report. Extends: "Conversation started by the agent" in `docs/product-map.md`. Owner
decision recorded in the pull request: a named `ai` overrides the person's pin for background team work only, never for the
parent itself. What's-new: none beyond the `leader-size-rule` entry (a person notices only the result); say so in the body.
Review: the other vendor. Cost: none at rest (two optional fields); a started AI process counts under the existing cap of 3.

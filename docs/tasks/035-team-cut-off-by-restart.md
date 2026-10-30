# A team member cut off by closing Faber is reported to its leader, who reopens it

Area: Conversations talking to each other
Files: engine/src/engine.rs, engine/src/engine/siblings.rs, engine/src/sibling.rs, engine/tests/engine.rs, docs/product-map.md
Depends: none
Size: about 200 changed lines

Why: the owner wants the team to keep going without a message from him, including after the PC restarts or Faber updates. The
leader hears from a member only through `Core::tell_parent` (`engine/src/engine/siblings.rs`), called when a member's turn ends
(`engine/src/engine.rs` near the end of the turn handler, `compact.rs`, `verifying.rs`, `by_itself.rs`). A reply that was
streaming when Faber closed is turned into `Status::Stopped` the next time its conversation is loaded (`settle()` in
`engine/src/engine.rs`), and nothing calls `tell_parent` for it: the member sits stopped, the leader waits for a report that
never comes, and the whole team stalls silently. The pending-report path survives a restart (`pending_parent_report`,
`restore_parent_reports`, `refill_parent_reports`) but only for reports that were already due. Verify the stall first with a
test that closes an engine with a streaming member and opens it again.

Build: at start-up (where `restore_parent_reports` is called in `engine/src/engine.rs`) every conversation an agent started whose
last reply was left streaming by a closed Faber, whose parent is not archived, is marked as owing a report (`pending_parent_report`),
and the existing refill sends it. It needs its own wording: `sibling::State` gets `Interrupted`, whose `said` line is "was cut off
when Faber closed, nothing was lost that is on disk", and `sibling::finished` tells the leader it may reopen the member with
`ask` ("Continue where you stopped. Check the project's state first") or start the work again; a stop by the person keeps today's
"was stopped by the person" and reports nothing as today. A member that finished its turn before the close is not reported again
(the report id `report-<conv>-<reply>` already deduplicates). Also write the same sentence into the tool description of `ask`
(one clause).

Leave out: restarting the CLI of a member by itself (the leader decides with `ask`), a leader that was itself cut off mid-turn (it
wakes when the next report arrives; say in the pull request what happens when no member is left, and add a follow-up task
if that case stalls), any screen change.

Done: an engine test that fails today: a member left streaming, engine reopened, the leader receives one message from Faber naming
the member and saying it was cut off, and a second reopening sends nothing more; a member stopped by the person's Stop sends
nothing. Extends: "Conversations talking to each other" in `docs/product-map.md`. What's-new: none (the team just keeps going).
Review: the other vendor. Cost: one scan of the conversation files already read at start-up; none at rest.

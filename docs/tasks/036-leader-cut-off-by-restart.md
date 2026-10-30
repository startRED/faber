# A team leader cut off by closing Faber goes on by itself when no member is left to wake it

Area: Conversations talking to each other
Files: engine/src/engine.rs, engine/src/engine/siblings.rs, engine/src/engine/by_itself.rs, engine/tests/engine.rs, docs/product-map.md
Depends: team-cut-off-by-restart, restart-says-it-closed-and-offers-to-continue
Size: about 150 changed lines

Why: the owner wants the team to keep going without a message from him after the PC restarts or Faber updates. Since
team-cut-off-by-restart, a member cut off by the close reports it to its leader the next time Faber opens, and that report
wakes a leader that was itself cut off mid-turn. When every member had already finished (their reports were taken in
before the close) or the leader had none running, nothing wakes it: its reply shows "Faber was closed while this ran" with
a Continue button (restart-says-it-closed-and-offers-to-continue), and the team stalls until the person clicks it.

Build: at start-up, a conversation that started others (some conversation's `started_by` names it), whose last reply has
`Message.closed` set, that has nothing queued and Keep going off, and none of whose members is working or owes a report,
is sent the same fixed continue turn the Continue button sends (`engine/src/engine/closed.rs`), once, as Faber's own
message. It counts against the same three-in-five-minutes limit, so a crash loop never spins. A person's own conversation
with no members never continues by itself (the button stays theirs).

Leave out: members (team-cut-off-by-restart reports them), any screen change, Keep going (it resumes by itself already).

Done: an engine test that fails today: a leader left streaming with every member finished, engine reopened, the leader gets
one continue turn and a second reopening sends none; a conversation with no members gets none. Extends: "Conversations
talking to each other" in `docs/product-map.md`. What's-new: none. Review: the other vendor. Cost: one pass over the
conversations already scanned at start-up; none at rest.

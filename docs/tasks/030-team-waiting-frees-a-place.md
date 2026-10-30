# A team conversation waiting for the person does not use up a place of the team

Area: Conversation started by the agent
Files: engine/src/engine/siblings.rs, engine/src/sibling.rs, engine/tests/engine.rs, docs/product-map.md
Depends: none
Size: about 150 changed lines

Why: the team must keep moving while the owner sleeps, and only the decisions are his. `Core::start_sibling`
(`engine/src/engine/siblings.rs`) counts as "running" every conversation an agent started whose turn is open
(`live.turn.is_some()`), has messages queued, is still empty or ends in a streaming reply. A conversation that asked the person
a question (`AskUserQuestion` or a permission card) keeps its turn open, so it counts: three members waiting on three questions
block the leader from starting anything else (the refusal reads "three conversations started by agents are already working in
this project") until the person answers, even though the leader could keep the rest of the job going. `state_of` in the same file
already tells `State::Waiting` from `State::Working`.

Build: the cap of `sibling::MAX_RUNNING` (3) counts the conversations that are `Working`; a `Waiting` one counts against a
second, larger limit on all open ones (working plus waiting, 5; name it `MAX_OPEN` in `sibling.rs`), because each is still a CLI
process. Measure first what one idle waiting CLI process holds (the worker-memory page of the app, `engine/src/engine/worker_memory.rs`,
reads owned process memory) and write the figure in the pull request; if five open processes cannot stay under the R125 budget
with the app's idle floor, lower `MAX_OPEN` and say why. Say in the refusal sentences which limit was hit (working, or all open)
and, when members wait, that they wait for the person, so the leader tells the person once through `wrap_up` instead of polling.
`list` and `read` already say "is waiting for the person to answer it"; keep those words.

Leave out: answering the waiting question for the person (Keep going's ten-minute auto-answer stays as it is), raising the
working cap.

Done: tests that fail today: with 3 members waiting for the person a fourth `start` is accepted; with 3 working it is still refused
with today's sentence; with 5 open (2 working, 3 waiting) the next is refused with the new sentence. Extends: "Conversation
started by the agent" in `docs/product-map.md` (the cap sentence). No what's-new entry (the leader's work keeps going; nothing new
to see). Review: the other vendor. Cost: up to two more idle CLI processes in the worst case; states the measured figure.

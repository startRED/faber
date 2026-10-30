# A big request makes the project agent lead a small team, a small one stays direct

Area: Agent's plan and closing box
Files: engine/src/persona.rs, engine/src/claude.rs, engine/src/codex.rs, engine/src/engine.rs, engine/tests/persona.rs, docs/product-map.md
Depends: leader-eval
Size: about 200 changed lines

Why: the owner's first product task: "a big request (several pull requests) makes the agent a manager by itself: it breaks the
request into tasks, opens team conversations, spreads them over the AIs by quota, keeps everything moving without a message
from me; a small request is done directly. Respect R160/R161: no mission board, no fixed roles." What a lead session does for
Faber's own development is the model: it splits a request into tasks of one pull request each (one idea, 100 to 400 changed
lines), runs a few team conversations that each take the next task, reviews with the other AI, watches the quota, and talks to
the person only for decisions, blocks and finished stages.

The tools already exist: `conversation` `start`/`read`/`list`/`ask` (`engine/src/sibling.rs`, `engine/src/engine/siblings.rs`:
the brief becomes a project conversation, at most 3 started by agents running per project, one level only, the finished
conversation's answer reaches the parent by itself through `tell_parent`), `plan` with `estimate`, `status`, `wrap_up`
(`engine/src/progress.rs`), `helper` (the other AI reviews or does a piece, `engine/src/helper.rs`), `quota`
(`engine/src/toolbox.rs`). What is missing is the decision and the habit: `engine/src/persona.rs` says nothing about when to
split (the only related line is `BIGGER_OFFER`, which offers a larger version of a small request), and the tool descriptions
only say "use it for independent work that takes a while".

Build: a `LEADS` text in `engine/src/persona.rs`, appended next to `BIGGER_OFFER` (the same two places, the `--append-system-prompt`
of `engine/src/claude.rs` and the developer instructions of `engine/src/codex.rs`) only for the project agent that may start
conversations: a project conversation that no agent started (`started_by` empty; a team member, a helper, a quick question
and a standing goal's fix never get it). A new launch flag carries that fact from `engine/src/engine.rs` where
`siblings::offers(area)` is read for the session. Bump `persona::VERSION`. The words, plain and short, tell the agent:
- A request is big when it holds more than one change that can be built, checked and reviewed alone (roughly more than one idea,
  or more than about 400 changed lines, or work in separate areas of the project). Everything else is done directly, with no
  team and no plan beyond the usual.
- For a big request: send `plan` with one step per task (each one idea, about 100 to 400 changed lines, ordered by what depends
  on what, the independent ones first) and an `estimate`; call `quota` and start up to three conversations with `conversation`
  `start`, each with a brief that says the goal, the files, what done looks like and the project's own rules for finishing a
  change (its AGENTS.md or README says how to check and hand in a change); keep your own context small; when a report arrives
  mark the step, then give that conversation its next task with `ask` or start a new one; have each finished piece looked at by
  the other AI with `helper`; finish with `wrap_up`.
- Talk to the person only for a decision that is theirs (direction, taste, money), something blocked, or a finished stage, in
  plain words; never ask them to say "continue", and never write "pull request", "branch" or "commit" to them (the persona's
  plain-words rule already holds).
No mode, no setting, no board and no named roles (R160, R161).

Leave out: choosing the AI per conversation (task `team-start-options`), anything in the screen, any setting.

Done: a test in `engine/tests/persona.rs` (or where `BIGGER_OFFER` is tested) that fails today: the leader text is in the
instructions of a project's own agent for both CLIs and absent for a team member, a helper, a quick question, Chat and a goal's
fix. The `leader_eval` example (task `leader-eval`) is run again, both vendors, and its table is in the pull request body: at
least 5 of 6 big requests start 2 or more conversations and 6 of 6 small ones start none, on each vendor; if a vendor misses,
tighten the words and run again before queueing. Extends: "Agent's plan and closing box" and "Conversation started by the
agent" in `docs/product-map.md` (one sentence each: when the agent splits). What's-new entry in every language ("Big requests:
the agent splits the work and runs a small team by itself"). Review: the other vendor, plus the coherence review of a change a
person notices (the taste cards whose "Touches" match, C011 among them). Cost: the instructions grow by about 400
tokens per project conversation; state it from the real text.

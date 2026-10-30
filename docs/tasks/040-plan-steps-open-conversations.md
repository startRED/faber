# The leader's plan shows which conversation does each step, and opens it

Area: Agent's plan and closing box
Files: engine/src/model.rs, engine/src/progress.rs, engine/src/engine/plan.rs, engine/src/engine/siblings.rs, ui/Plan.tsx, ui/api.ts, tests/ui/, ui/locales/, docs/product-map.md, docs/design/
Depends: none
Size: about 350 changed lines

Why: the owner wants the work the agent hands out to show as conversations or steps the person can open, not as a mission board
(R160, R161). Today the sidebar already nests a team member's conversation under its leader (`ui/sidebar.ts` `nest`,
`startedBy`), and the leader's plan draws under the newest reply as "Plan · 2 of 5" (`ui/Plan.tsx`, the `plan` kept on the
conversation, `engine/src/progress.rs`). The two are not joined: the person cannot see which step is running in which
conversation, or whether it waits for them, without opening each row. The plan is the one list the agent already writes, so the
join belongs there and not in a new screen or board.

Build: `PlanStep` (`engine/src/model.rs`) gets an optional `conversation` id (`skip_serializing_if` none, so every saved file
opens in any version). The `plan` tool's step schema takes it (`progress.rs`, `plan_schema` and `plan()`), cut and validated: it must
be a conversation of this project that this conversation started (`engine/src/engine/plan.rs` `write_progress` has the core;
a wrong one is answered as an error the agent reads, like the other plan errors). `shown_plan` adds, for each such step, the
member's title and live state (working, waiting for you, finished, failed, stopped, from `state_of` in `siblings.rs`), and the
plan event is sent again when a member changes state (the same place the running column refreshes: `refresh_running`). In
`ui/Plan.tsx` such a step shows the title as a quiet link and a short state word under the step's text; a click opens that
conversation through the existing open action (do not add a route). Reuse the state words the running column and sidebar use
(`ui/locales/`, find them with the existing "needs you" and "working" keys); add a key only where none fits, in every language by
hand. Tell the agent in the `plan` description to pass the id of the conversation it started for a step.

Leave out: a board or columns, assigning from the screen, dragging, any new area. The row stays one line per step, reduced motion
and the 720 px window as for the rest of the plan.

Done: an engine test that fails today (a plan step with a started conversation's id keeps it, a foreign id is refused, the state
changes when the member asks the person) and a screen check in `tests/ui/` that draws the plan with two linked steps and opens
the conversation on click (extend the check that already draws the plan: grep `Plan ·` in `tests/ui/`). Extends: "Agent's plan and
closing box" in `docs/product-map.md` and the plan part of the thread page in `docs/design/`. What's-new entry in every language
("The plan shows which conversation works on each step, and opens it"). Review: the other vendor and the coherence review (cards
checked named in the body); a photo of the screen at 720 px in the longest language. Cost: the plan grows by one short string per
step and one event per member state change; states the numbers.

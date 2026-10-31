# Read an Antigravity run's output into Faber's reply events

Area: none
Files: engine/src/agy.rs, engine/src/lib.rs, engine/tests/agy_parse.rs
Depends: agy-recorded-runs, gemini-vendor-variant
Size: about 380 changed lines

Why: a Gemini conversation needs its stream turned into what Faber shows: text as it arrives, steps (commands, file changes,
reads, searches), usage, the end and errors. Both existing drivers do this as a pure step before any process work:
`engine/src/claude.rs` reads Claude Code's stream lines and `engine/src/codex.rs` the app-server's messages into the events
that `engine/src/events.rs` (`Session`) delivers to the engine, and each is tested by replaying recorded files
(`tests/fixtures/claude/`, `tests/fixtures/codex/`). The Antigravity CLI's own format is recorded by task `agy-recorded-runs`
(`tests/fixtures/agy/`, `docs/dev/antigravity-cli.md`): lines `{"event":"init"|"step_update"|"result", ...}`.

Build `engine/src/agy.rs` with the parser only, no process: a stateful reader that takes one line and returns the events the
engine already knows (copy the shapes the Claude reader emits for text deltas, tool starts and ends with the same tool names the
screen already draws where they match: command, file change, read, search; unknown `step_type`s become a generic step with its
name, never dropped), the usage totals (input, output, thinking, cache read) in the units `usage.rs` expects, the session id for
resume (`conversation_id`), and the end of a turn with its status. Errors: a line that is not JSON is ignored once and logged
once; a `result` whose status is not success ends the reply with a `reason.*` key from the existing set, no new sentence. The
reply is bounded the way the other drivers bound it (a step's text and a command's output are cut at the same limits).

Leave out: starting a process, permission cards, Faber's tools, quota, routing (the next tasks).

Done: `engine/tests/agy_parse.rs` replays every file of `tests/fixtures/agy/` through the parser and compares the event lists with
checked-in expectations; fails today (no module). A property check: any prefix of any fixture parses without a panic. Extends: the
"Gemini (Antigravity)" row of `docs/product-map.md` (built: the reader). No what's-new entry. Review: the other vendor. Cost: none
at rest; the parser holds one reply's state.

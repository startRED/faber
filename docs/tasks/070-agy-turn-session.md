# A Gemini conversation in a project runs a turn, stops, and resumes

Area: Projects
Files: engine/src/agy.rs, engine/src/engine.rs, engine/src/proc.rs, engine/src/events.rs, testkit/src/fake.rs, engine/tests/agy_session.rs, docs/product-map.md
Depends: agy-reply-parser
Size: about 400 changed lines

Why: with the variant (`gemini-vendor-variant`) and the parser (`agy-reply-parser`), a team conversation can only run on Gemini once
a process starts it. Sessions are built in `engine/src/engine.rs` (`match pick.vendor { Vendor::Claude => start_claude(...),
Vendor::Codex => ... }`) and all return `events::Session`, which the engine drives without knowing the AI. Child processes never go
through a shell and are owned by a job so they die with the turn (AGENTS.md; `engine/src/proc.rs`, `winproc/`); Windows is the
first target and macOS only keeps compiling.

Build `agy::start_agy(options, sink) -> Session` for Projects only (Chat and Auto's router do not offer Gemini yet): the command
line from the facts recorded in `docs/dev/antigravity-cli.md` (print mode with stream-json in and out, `--model`/`--effort`
from the pick, the working folder, `--add-dir` for the project's other folders as the other drivers pass them, isolation settings
so the owner's own memory, skills, plugins and MCP servers stay out), Faber's permission modes mapped to what the CLI supports
(Full access to `--dangerously-skip-permissions`, Accept edits to `--mode accept-edits`, Plan to `--mode plan`, Auto to `--mode
auto`; Ask, which needs a card the CLI cannot raise in print mode if the recording shows that, runs as Accept edits and the
screen says so, exactly as Auto already does for a CLI without an auto mode), the prompt on stdin or in the argument as recorded,
the resume of a conversation with `--conversation <id>` from the id the parser returned, Stop (the interrupt and the end of the
process tree, no console window on Windows), a turn that ends, and a process that dies. The session offers no Faber tool
(`conversations`, `helper`, `plan` flags false; a Gemini member reports in its final answer, which `tell_parent` already carries to
its leader); say so in the product map. Replace the error key of `gemini-vendor-variant` with the real start.

Leave out: Faber's tools for Gemini (a later task if the recorded MCP facts allow it), sign-in and setup (`agy-sign-in-and-setup`),
quota and routing, the screen.

Done: `engine/tests/agy_session.rs` runs a project conversation on the fake `agy` replaying `tests/fixtures/agy/`: a text turn, a
turn with a command and a file change, Stop mid-turn leaves no process, a second message resumes the conversation with the
recorded id, a process that exits early ends the reply with an error key. Fails today (cannot start). Extends: the "Gemini
(Antigravity)" row of `docs/product-map.md`. No what's-new entry until the person can pick Gemini. Review: the other vendor, plus a
second security review (a process launcher with a no-prompt flag): both before queueing, per R146. Cost: one CLI process per Gemini
conversation, counted by the worker-memory registry like the others (`engine/src/engine/worker_memory.rs`); states the measured
process memory of an idle `agy` session.

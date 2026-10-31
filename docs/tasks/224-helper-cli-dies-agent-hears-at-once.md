# A helper whose CLI dies mid-work answers the agent's call at once, in both directions

Area: Helper from the other AI
Files: engine/src/engine/helpers.rs, engine/tests/engine.rs, tests/fixtures/claude/, tests/fixtures/codex/, docs/product-map.md
Depends: none
Size: about 200 changed lines

Why: the helper tests cover reports, messages, quota stop and background settling, but none ends a helper by its own process dying. Claude ends with `Claude Code stopped (exit n)` (`claude.rs`) and Codex with `Codex stopped.` (`codex.rs`). If either does not reach `end_helper`, the agent waits up to `MAX_RUN`, 30 minutes. The leader agent depends on this in both directions. First check the paths; fix only what fails.

Build: tests with a fake helper process that exits abruptly, after some words and without any, in four cases: a Claude agent with a Codex helper and the reverse, each with a partial report and an empty one. Add a `helper-dies` fixture per vendor. Fix any case that waits.

Done: the tests assert the agent's call ends within 2 s of the exit with failure text that keeps the partial words, the cost is noted as unknown, the helper count falls to zero, and no process stays behind. Since this may pass today, the proof is a mutation: cutting the exit-to-end path turns the tests red. Cost: none.

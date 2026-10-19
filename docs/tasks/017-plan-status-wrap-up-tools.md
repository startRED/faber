# The agent shows its plan, what it is doing now, and a closing box

Build parts 2, 3, 4 and the prompt half of part 6 of [the design](../superpowers/specs/2026-10-19-legible-agent-runs-design.md#2-the-plan-the-person-can-see-task-017):
- Three tools on Faber's MCP server (engine/src/helper.rs), for Claude Code and Codex alike:
  - `plan`: the conversation's checklist;
  - `status`: 3 to 6 words of what the agent is doing now;
  - `wrap_up`: a 2 to 3 sentence recap, one optional question with one or two buttons, and a `done` flag.
- How they are drawn: under the newest reply, in the running column and in the sidebar's status.
- Under Keep going, `wrap_up` with `done` stops the run in place of the `[keep going: stop]` text line. The old
  line stays readable for one release and is always stripped.
- The narration rules go in engine/src/persona.rs.

This changes how Keep going stops, so it names R199 and gets a new decision.

Done:
- a fixture per tool, replayed by engine/tests/ and tests/ui/;
- a Keep going run that stops through `wrap_up`;
- the product map rows for the new tools and words; docs/design/keep-going.md and layout.md;
- a what's-new line in every language;
- memory measured on the queued commit.

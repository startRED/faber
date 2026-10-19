# What the agent left running shows as chips, and followed PRs show their state

Build parts 7 and 10 of [the design](docs/superpowers/specs/2026-10-19-legible-agent-runs-design.md#7-background-work-as-chips-task-019):
- One row of quiet chips under the plan for what the agent left running: background shells, monitors, helpers,
  Faber's checks and followed pull requests. Each shows running, passed, failed or waiting. They are read from the
  tool calls Faber already parses.
- A pull-request number or commit Faber follows renders as a chip with its live state; an unknown one stays plain
  text.

Needs 017's plan row.

Done:
- a fixture with a background shell, a helper and a followed PR, replayed by engine/tests/ and tests/ui/;
- the product map and layout.md say what changed;
- a what's-new line in every language.

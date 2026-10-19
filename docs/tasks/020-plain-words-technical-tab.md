# Plain words in the conversation, and a "Technical details" tab

Asked by Erick on 2026-10-19: the conversation has to use plain words without technical terms ("CI ficou verde" and
the like); merges, commits, tests and the rest belong in another tab.

Build part 11 of [the design](../superpowers/specs/2026-10-19-legible-agent-runs-design.md#11-plain-words-and-a-technical-tab-task-020):
- the plain-words rule in engine/src/persona.rs for both CLIs, which also covers the closing box, the done card and the
  tray notice;
- a "Technical details" view per project conversation in the running column. It lists what is running, the changes
  saved and their state, the checks run, the files changed and the reviews, newest first, all filled from what the
  engine already parses;
- one plain line in the thread that opens it.

Can start right after 016; 019 then fills its running list and live states.

Done:
- a recorded run whose reply is in plain words, replayed by engine/tests/;
- a tests/ui/ check that opens the tab from the thread's line and shows a saved change and a check;
- the product map and docs/design/layout.md say what changed;
- a what's-new line in every language;
- memory measured on the queued commit.

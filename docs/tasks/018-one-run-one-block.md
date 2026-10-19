# A Keep going run reads as one block

Build parts 5, 9 and the screen half of part 6 of [the design](../superpowers/specs/2026-10-19-legible-agent-runs-design.md#5-one-run-one-block-task-018):
- Consecutive automatic turns render as one run: one header with the time, the turns and the plan's progress. The
  turns stay folded and empty turns are not drawn.
- The questions Faber answered become one card with "Change".
- The narration inside the work log folds to its last two paragraphs.

Start after 017 if it is close: the run header shows the plan.

Done:
- a recorded long run (tests/fixtures/) replayed by tests/ui/ shows one header, no empty turns, and the answers
  card;
- the product map and docs/design/keep-going.md and layout.md say what changed;
- a what's-new line in every language.

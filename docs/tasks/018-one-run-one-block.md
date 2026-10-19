# A Keep going run reads as one block, and repeated notices show once

Build parts 5, 8, 9 and the screen half of part 6 of [the design](docs/superpowers/specs/2026-10-19-legible-agent-runs-design.md#5-one-run-one-block-task-018):
- Consecutive automatic turns render as one run: one header with the time, the turns and the plan's progress. The
  turns stay folded and empty turns are not drawn.
- Identical notices in a run show once with ×N.
- "Não foi possível medir partes deste turno." moves to a "≈" before the cost.
- "Interrompida." shows only for the person's own Stop.
- An empty "O que falhou" says "no details".
- The questions Faber answered become one card with "Change".
- The narration inside the work log folds to its last two paragraphs.

Start after 017 if it is close: the run header shows the plan.

Done:
- a recorded long run (tests/fixtures/) replayed by tests/ui/ shows one header, no empty turns, one ×N notice and the
  answers card;
- the product map and docs/design/keep-going.md and layout.md say what changed;
- a what's-new line in every language.

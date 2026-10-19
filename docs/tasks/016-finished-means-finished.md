# A finished reply looks finished, and a done card waits in the running column

Asked by Erick on 2026-10-19 (screenshot at 11:24). A reply's text had ended, but four things still said it was
running: the header read "Trabalhando", the composer showed Parar, a white caret blinked, and the running column said
"Claude trabalhando sozinho 4 min". His words: when a run ends, the running column should show a card saying it
finished; a click opens the conversation and the card goes, and an × removes it, "like Claude and ChatGPT desktop".

Build part 1 of [the design](docs/superpowers/specs/2026-10-19-legible-agent-runs-design.md#1-finished-means-finished-task-016):
- find which signal kept the reply "working" after its text ended, and fix it with a recorded fixture;
- draw the caret only while text arrives;
- add the done card in the running column, the unseen dot on the sidebar row, and one stored "ended, not seen" mark
  that both of them and the tray notice of task 015 read.

Do 015 first or together with this: it shares the mark.

Done:
- engine/tests/ and tests/ui/ replay the stuck case and fail without the fix;
- a check opens and dismisses a done card;
- the product map's Running column, Keep going and Tray rows, docs/design/keep-going.md and layout.md say what
  changed;
- a what's-new line in every language.

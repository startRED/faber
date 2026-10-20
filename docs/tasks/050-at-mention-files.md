# `@` points at a file or folder of the project

Parity row T4 ([parity.md](../product/parity.md#talking-and-steering)). Typing `@` in the composer of a conversation
with a working folder lists the folder's files and folders, filtered as you type (git-ignored ones left out); Enter or a
click puts the path in the message, the way the CLIs read it. Extends the Composer row of the product map; no new
setting. The list comes from the engine, never from a shell, and stays small (the first 50 matches).

Done:
- tests/ui/ checks the list opens, filters, inserts the path and closes with Esc; engine tests check the ignored files;
- the product map's Composer row, docs/design/ for the composer, and parity.md without T4;
- a what's-new line in every language, and the commit states the memory cost.

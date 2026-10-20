# The whole change of a conversation as a diff

Parity row U2 ([parity.md](../product/parity.md#undo-and-checking)). From a project conversation that changed files,
one control opens in the running column (like a subagent or a page) the files it changed, each with its added and
removed lines, read from git in its working copy; no second column. Extends the work log's line counts. Large diffs
load per file when opened.

Done:
- tests/ui/ with a recorded conversation that changed two files shows both and their lines;
- the product map and docs/design/layout.md (running column), parity.md without U2, the memory cost, and a what's-new
  line in every language.

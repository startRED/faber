# Go back to before a message

Parity row U1 ([parity.md](../product/parity.md#undo-and-checking)); "snapshots and undo" in the product map's "Not
built yet" and docs/product/areas.md. Faber snapshots the files of the working folder before each turn (git-aware, only
what the turn touches, both CLIs, commands included, which Claude Code's own rewind misses), and the person can go back
to before a message: the files, the conversation, or both, with a way to redo. Write the design first as a spec in
docs/superpowers/specs/ (what a snapshot holds, where, how big, how long it is kept, how it meets separate copies and
helpers) and take its direction questions to the owner in one card; build after.

Done:
- the spec approved; engine tests restore files and conversation from recorded fixtures; tests/ui/ checks the control;
- the product map, parity.md without U1, the disk and memory cost in the commit body, and a what's-new line in every
  language.

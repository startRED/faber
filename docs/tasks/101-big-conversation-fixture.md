# Add a synthetic 100 MB conversation to measure open and save times

Area: none
Files: engine/tests/support/mod.rs, engine/tests/big_conversation.rs, engine/tests/all.rs
Depends: none
Size: about 220 changed lines

Why: `docs/research/2026-10-29-faber-diagnosis/README.md`, part 1. The owner's biggest conversation is 102 MB (1,362
messages, 35,902 tool records) and the other storage tasks (the compact file, tool input in pages, the newest messages
first, the save outside the lock) each promise a time or a size, but none can be proven on the owner's private file.
They need one shared stand-in of the same shape, built in a temporary folder, never copied from his data.

What to build: a helper in `engine/tests/support/mod.rs` that writes a `Conversation` (`engine/src/model.rs`) of a stated
size into a temporary data directory through `Store::save_checked` (`engine/src/store.rs`): N messages, most of them
assistant messages with many `ToolUse` records, where about 70 % of the records belong to helpers (`parent` set) and each
has `input` and `output` (the engine cuts `output` at 4000 characters, `engine/src/engine.rs` near line 2511, so use
that cap and a few large `input` values such as a written file). Then a test file `engine/tests/big_conversation.rs`
(added to the `mod` list of `engine/tests/all.rs`) that builds the 20 MB and the 100 MB cases and prints, once, the
time of `Store::get`, of `Store::save_checked`, of `Engine::open_for_screen` and the file size. It asserts only loose
bounds (the 100 MB case saves and reopens equal to what was written) and prints the numbers; later tasks tighten them.

Leave out: any change to the engine itself, the owner's files, a build over 1 GB of memory (the 100 MB case builds its
messages in a loop and drops them).

Done: the new test passes and prints the four numbers for both sizes in the commit body; it fails if the 100 MB
conversation does not reload equal. Extends: no product-map entry (tests only). No review beyond the lints (tests only).

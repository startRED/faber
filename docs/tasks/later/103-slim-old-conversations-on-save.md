# Move the large inputs of already saved conversations into pages on their next save

Area: Other stored state
Files: engine/src/engine.rs, engine/src/store.rs, engine/tests/big_conversation.rs
Depends: big-tool-input-in-pages
Size: about 250 changed lines

Why: `docs/research/2026-10-29-faber-diagnosis/README.md`, part 1. After `big-tool-input-in-pages` only new tool
calls keep a short record; the owner's four conversations over 10 MB (102, 45, 17 and 15 MB) were saved the old way and
stay as heavy as ever. The first task must have kept reading old files unchanged ("read the old field, write the new
shape"); this one writes the new shape for them.

What to build: when `Core::load` brings a conversation in from disk and any `ToolUse` has an `input` over 4 KB and no
`detail`, move it to pages and cut the record the same way (a helper of `big-tool-input-in-pages`, not a second
implementation), then save once, in the background and never on the opening path (use the coalesced save of PR #445,
`save_soon`). Do it per conversation, only when it is opened, never as a sweep of the data folder. It must be safe to
stop at any point: the pages are written first and the record second, so a crash leaves either the old record or the
new one, never a record that points at missing pages (`Store::save_checked` already replaces the file atomically).
Print one line to the engine log with the bytes saved.

Leave out: files that are not opened, the tool output (already cut at 4000), compacting the file text (task
`write-conversations-compact`).

Done: a test writes a conversation in the old shape (inputs inline) to disk, opens it, waits for the save and checks the
file is smaller, every input is still readable through `tool_detail`, and a crash between the pages and the record
(use the `Disk` fake of `engine/tests/failed_writes.rs`) leaves the old record readable. Extends: product-map "Other
stored state". The commit states the memory and processor cost (R125). Review by a fresh session of the other vendor
(R146).

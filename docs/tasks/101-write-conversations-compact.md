# Write conversation files without indentation

Area: none
Files: engine/src/store.rs, engine/tests/store_summaries.rs
Depends: none
Size: about 120 changed lines

Why: `docs/research/2026-10-29-faber-diagnosis/README.md`, part 1: one rewrite of the owner's 102 MB conversation costs
about 0.8 s. `Store::write_with` (`engine/src/store.rs`, near line 261) serializes with `serde_json::to_string_pretty`,
which adds an indent and a newline to every field of 35,902 tool records. Compact JSON is the same data in fewer
bytes, so serializing, writing and flushing all get cheaper with no change of shape.

What to build: serialize conversation files with `serde_json::to_vec` (keep pretty output for the small settings,
project and ledger files, which a person may read, by choosing per call: only the conversation path changes). Reading
accepts both, because `serde_json` parses either. Before editing, grep every reader of a conversation file as text
(`Store::mentioning` calls `contains` on the file text; `search.rs`; the tests that compare file text) and keep each
working. Measure the same 20 MB synthetic conversation before and after (use `101-big-conversation-fixture` if it has
landed, else a local loop of the same shape) and write both numbers in the commit body.

Leave out: any new file format, compression, or a change to what the conversation holds.

Done: a test that saves one conversation and checks the file has no line break between fields and loads equal, and that
an old pretty file still loads and is rewritten compact on its next save; the commit states the size and time before and
after and the memory cost (R125). Extends: product-map "Other stored state" (the `conversations/<id>.json` row says how
the file is written). Review by a fresh session of the other vendor (R146), because it touches data on disk.

# Keep large tool input out of the conversation file

Area: Work log
Files: engine/src/engine.rs, engine/src/model.rs, engine/src/transcript.rs, engine/tests/transcript.rs, engine/tests/big_conversation.rs
Depends: edit-counts-from-the-engine
Size: about 400 changed lines

Why: `docs/research/2026-10-29-faber-diagnosis/README.md`, part 1. The conversation file holds every tool's `input`
(`ToolUse.input`, `engine/src/model.rs` line 124) whole: a written file, the old and new text of an edit, a long
prompt to a helper. The output is already cut at 4000 characters (`engine/src/engine.rs` near lines 2511 and 2535), so
input is what is left of the 92.6 MB. `TranscriptStore` (`engine/src/transcript.rs`) already keeps bounded immutable
pages for a helper's transcript (`append_page`, `read_page`, `copy_calls`, `remove_conversation`).

What to build: in `Core::handle` for `EngineEvent::Tool`, when the serialized `input` is over 4 KB, write the full JSON
text as pages for that call (the same store and the same `(conversation, call)` key) and keep in the record only a short
preview of each long string leaf plus `ToolUse.detail: Option<{ pages, bytes }>` (`serde(default)`, old files load
unchanged). Keys the engine itself reads from `input` must stay whole in the record: `command` (`engine/src/nudge.rs`
near line 183), `file_path` (line 196), `prompt` and `brief` of a helper (`engine/src/engine/facts.rs` line 27), and
`stuck.rs` compares inputs (line 58, hash the whole input before cutting it). Add `Engine::tool_detail(id, call)` that
returns the full input from the pages; `Engine::branch` and the conversation copy already copy pages of calls that
have a `transcript` (`engine/src/engine.rs` near line 4558), so make them copy a call with `detail` too, and
`remove_conversation` already clears them on delete. The edit counts come from `edit-counts-from-the-engine`, so the
screen no longer needs the text.

Leave out: Codex `fileChange` output, helper transcripts (they already use pages), old files (next task).

Done: a test shows a 5 MB `Write` input leaves a record under 5 KB, `tool_detail` returns the whole text, a branch keeps
it, deleting the conversation removes the pages, and the nudge and stuck checks still see their keys; the 100 MB
fixture's file size drops and the commit says by how much and the memory cost (R125). Extends: product-map "Work log"
and "Other stored state" (the page folder now holds main-agent calls too). Review by a fresh session of the other
vendor (R146).

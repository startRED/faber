# After Go back, the AI continues in its own session cut at that message

Area: Go back before a message (task 090)
Files: engine/src/rewind.rs, engine/src/engine/rewind.rs, engine/src/claude.rs, engine/src/codex.rs, engine/tests/rewind.rs, tests/fixtures/, scripts/cli-qualification.mjs, docs/design/rewind.md, docs/product/areas.md, docs/product-map.md
Depends: none
Size: about 300 changed lines once qualification fixes the contract; prove the CLI side first, then wire it

Why: "Go back before this message" restores files and the conversation, but `SavedConversation::apply` in `engine/src/rewind.rs`
clears every native session ("Native rewind semantics are unproved. Both restore and redo continue from saved messages"),
so the next turn starts a new CLI session that gets the visible history as text. The AI loses its native context (what it
read and ran) and gets the visible history as text instead (billing and cache effects are not measured), and the thread says "The AI will continue from the saved
messages". `docs/product/areas.md` ("Not built yet") lists native continuation after going back as missing.

Build:
1. Qualify the installed versions with recordings before any engine change. Codex 0.166.0's app-server exposes
   `thread/revert` with `threadId` and `beforeTurnId` (`codex app-server generate-json-schema`, `ClientRequest.json` and
   `v2/ThreadRevertParams.json`); Claude Code 2.1.312's help lists `--fork-session` but no flag that resumes a session cut at
   a message, so Claude needs a proven, supported cut mechanism or it stays on today's path. Add what qualifies to the
   contract probes of `scripts/cli-qualification.mjs` and record one fixture per qualified vendor in `tests/fixtures/` (the
   fake CLI replays it). Only contracts the recordings confirm are used.
2. The restore point keeps, per vendor, the native session id and the message or turn it was at (the snapshot already
   holds the messages; add only what the resume needs). A conversation-only or files-and-conversation restore that has
   them keeps the session and, on the next turn, resumes it cut there; the notice is gone in that case. Redo, a restore
   whose session id is missing or expired, a vendor switch since the point, and a failed native resume fall back to
   today's path once, with today's notice.

Leave out: helpers' and asides' sessions (they still end), rich rewind diffs, history limits, any screen change beyond
the notice not showing.

Done: native continuation ships for at least one qualified vendor (Codex is expected); if neither qualifies, the pull
request carries only the qualification evidence and splits the wiring into a follow-up task. An engine test in
`engine/tests/rewind.rs` that fails today: per qualified vendor, after Go back the next turn
resumes the recorded session at the cut (fake CLI checks the arguments or the `thread/revert` call) and sends only the new
message, not the history; a second test shows the fallback when the native resume fails. Update the "Go back" lines of
`docs/design/rewind.md`, the "Not built yet" line of `docs/product/areas.md` and the feature row in
`docs/product-map.md`. What's-new: one "better" line in every language (the AI remembers its work after going back).
Review: the other vendor (session handling nobody checks by reading once). Cost: none at rest; one saved id and position
per restore point.

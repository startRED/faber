# Name the Codex threads Faber starts with the conversation's title

Area: Chat conversation
Files: engine/src/codex.rs, engine/src/engine.rs, engine/tests/codex_session.rs
Depends: none
Size: about 150 changed lines

Why: the live look of 2026-10-29 (`docs/research/2026-10-29-faber-diagnosis/README.md`, "Live look"): Faber's Codex
threads show in the person's own Codex app history titled "Faber started your context fresh: nothing said ear…". That
is the handoff text of `engine/src/sibling.rs` (line 304), which becomes the thread's first user message after a clear,
and Codex names a thread from its first user message. The app-server has a request for it: `thread/name/set` with
`{threadId, name}` (checked on 2026-10-29 with `codex app-server generate-json-schema`, codex-cli 0.166.0: the name is
in `ClientRequest.json`). Today `engine/src/codex.rs` never sends it. Once the answer to `thread/start` or
`thread/resume` arrives (`Waiting::Thread(key)`, near line 1624, where the thread id is learned) the code already sends
`skills/list` and the queued turns.

What to build: `CodexThread` (`engine/src/codex.rs` line 770) gets `title: Option<String>`, set by the engine only for a
conversation the person sees (not for the routing question or any thread with `instructions`, which are internal), from
`Conversation.title` (set from the first message, `engine/src/engine.rs` near line 4438). After the thread opens, send
`thread/name/set` once, before the first `turn/start`, with that title cut to 60 characters; the answer is ignored,
and an error (an older Codex without the request) is not shown and not retried. A resumed thread is named too when it
has no name of Faber's. The person's own Codex history then shows the conversation's title, in their language because
it is the words they typed.

Leave out: renaming later when the title changes (a follow-up after `short-title-from-the-first-exchange` lands), Claude
sessions, anything that reads Codex's own files.

Done: a test in `engine/tests/codex_session.rs` with the fake app-server fails today because no `thread/name/set`
arrives; it passes with exactly one per thread, none for an internal side question, and no failure when the fake
answers with an error. Extends: product-map "Chat conversation" (one sentence: the Codex thread carries the title). No
what's-new entry (nothing in Faber's own screen changes). No memory or processor cost beyond one request per thread.
Review by the other vendor (R146).

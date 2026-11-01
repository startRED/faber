# The guide and kit digests are saved when the CLI first answers, not when the message is sent

Area: Conversations (guide and kits)
Files: engine/src/engine.rs, engine/src/engine/kits.rs, engine/tests/kits.rs, engine/tests/session_resume.rs
Depends: none
Size: about 150 changed lines

Why: two shortcut comments say a CLI that dies before reading the first message still counts as having been given its guide and kit: `engine/src/engine.rs:4038` ("saved when the message is dispatched, not when the CLI accepted it") and `engine/src/engine/kits.rs:175` (the kit's digest is saved even if its resources wait behind a `/` first message). After a crash or kill, the retry resumes the same native session and skips the guide, so the agent works without its rules.

Build: keep the digests in the live record when sending. Write them into `session_guide_digests` and `session_resource_digests` on the first event of that process's reply (or its end), and drop them if the process ends before it. Remove both shortcut comments.

Done: engine tests that fail today, once with the Claude and once with the Codex fixtures: a fake CLI that exits before any event, then a retry on the same session, sends the guide and kit resources again. A normal turn still sends them once only. Cost: none (one field in the live record).

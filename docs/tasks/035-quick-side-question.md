# Quick question on the side (`/btw`)

Build item 10 of [following.md](../product/following.md) as the owner approved it (R250, parity row T1): a "Quick
question" tab in the running column, opened also by `/btw <question>` in the conversation's box, that asks a read-only
copy of the conversation while the agent works, without stopping it or waiting in its queue.

- Claude Code: resume the conversation's CLI session with `--fork-session`, only Read, Grep and Glob allowed, so the
  main session is never touched; Codex: a new side request with Faber's record of the conversation, read-only sandbox.
- Nothing goes back into the conversation; "Send to the agent" puts an answer's text in the main box.
- One CLI process per question, gone when it answers; same model at low effort, with the tab's own picker.
- Edges: no turn yet, quota stop, a 720 px window (over the running list with a back arrow).

Done:
- `tests/ui/quick-question.check.ts` asks while a recorded turn runs and proves the main turn, its queue and its CLI
  session did not change; engine tests prove the fork and the read-only tool list for both CLIs;
- the product map, docs/design/layout.md (running column), following.md item 10 marked built, parity.md without T1, the
  memory and processor cost in the commit body, and a what's-new line in every language.

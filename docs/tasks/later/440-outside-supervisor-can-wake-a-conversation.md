# A supervisor outside Faber can wake a conversation (needs the owner's decision)

Area: host, cli
Files: `cli/commands.json`, `cli/src`, `host/src` command table, `docs/product-map.md`
Depends: none
Size: one pull request

Why: On 2026-11-15 another AI session watched Faber for a person who slept. The Faber command line only reads conversations (`asks`, `conversations`, `conversation`, `projects`, `quota`, `settings`, plus `answer_ask` with reach `person`), so the only way to wake a stalled conversation was to drive the screen with clicks and typing. That is slow and fragile (focus taken by a game, sidebar rows that move) and was needed several times in one night. Whether an outside session may send a message to a conversation is a question of permissions and trust, so it is the owner's decision, not a task to start unasked.

Build (after the owner decides): a `conversation_wake` or `conversation_send` command in the table with its own reach and a clear refusal for anything reserved to the person (permission answers, accounts, payments), recorded in the conversation as coming from outside.

Leave out: any way to read or send CLI credentials or session tokens; answering permission asks.

Done: Tests fail without the change: an allowed call wakes or messages a conversation and appears marked as outside; a reserved action is refused. The decision is recorded in `.faber/decisions.md` by the owner.

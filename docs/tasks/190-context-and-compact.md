# What fills the context, and compact now (parity C1)

Row C1 of docs/product/parity.md: the context meter (R221) opens a short breakdown of what fills the context (the
system part, tools, files and the conversation, as far as each CLI reports it) and offers "Compact now", passed to the
CLI's own compaction (Claude Code `/compact`, Codex's compact request) instead of building one in Faber. Extends the
Context meter; the `/` entries stay the same action.

Done when: engine tests show each CLI receives its own compact request; a screen check with the fake CLI opens the
breakdown and compacts; the product map and docs/design are updated; a what's-new entry in five languages.

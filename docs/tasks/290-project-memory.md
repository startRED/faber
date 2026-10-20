# Memory: what the AI learned, per project (parity X3)

Row X3 of docs/product/parity.md, with "Learning and memory" in docs/product/following.md: the person sees what each
CLI remembers for a project (Claude Code's project memory files, Codex's memories, as each CLI keeps them inside the
project, never the sign-in folders), where each line comes from, and edits or deletes a line. What belongs to a project
stays in it. Faber reaches each CLI's own memory first and builds its own only where a CLI has none. Extends the
project's page.

Done when: engine tests of reading, editing and deleting for both CLIs from fixtures; a screen check of the list and a
delete; the product map and docs/design are updated; a what's-new entry in five languages.

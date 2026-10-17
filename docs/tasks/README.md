# Task queue

What a session does next when it finishes a task, without asking (docs/design.md#task-queue). One file per task:
`<order>-<name>.md`, lowest order first (`010-`, `020-`; leave gaps to slot one in). The file holds a `# Title` line and
then the brief a fresh session needs: the task, the files or area, what done looks like. The owner decides what goes here
and in what order; a session may add a follow-up it found, after the owner's tasks.

`node scripts/tasks.mjs next` claims the first free task and prints it, `list` shows who holds what, `release <name>`
gives one back. The pull request that finishes a task deletes its file.

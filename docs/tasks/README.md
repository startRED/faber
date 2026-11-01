# Task queue

The queue holds only the tasks for Faber 1.0, with at most 15 active task files. The owner decides what enters and its order; only the owner, through the manager session, adds a task. Everything else waits in `docs/tasks/later/` in the same brief format. `tasks.mjs next` does not take tasks from `later/`. A task found while working goes there unless the owner moves it into the active queue.

One file per task: `<order>-<name>.md`, lowest order first. Each brief gives a fresh session the area, files, dependencies, size, reason, work, exclusions and proof of completion. Keep each under 6,000 bytes (`npm run docs`). A grouped brief can name two sequential pull requests when one would be too large.

`node scripts/tasks.mjs next` claims the first free active task whose dependencies have landed; `list` shows the claims and `release <name>` gives one back. The pull request that finishes a task deletes its file. The [board](https://github.com/startRED/faber/blob/board/BOARD.md) shows the list; [workflow](../dev/workflow.md#task-queue) explains how it works.

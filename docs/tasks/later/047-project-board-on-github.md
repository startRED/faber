# A person can publish their project's progress board to their own GitHub

Area: Settings screen
Files: engine/src/project/, engine/src/engine/plan.rs, ui/ProjectSettings.tsx, ui/locales/, tests/ui/, docs/product-map.md, docs/design/
Depends: project-progress-line, follow-ups-as-task-files
Size: about 350 changed lines

Why: slice 1 of `docs/superpowers/specs/2026-10-30-faber-method-2.md`. Faber's own repository already has its board
(docs/dev/workflow.md#board): BOARD.md, the only file of a parentless commit on the branch `board`, rebuilt from state and
never edited. A person's project gets the same.

Build:

- An option in the project's settings, off by default because it publishes to the person's repository: "Show progress
  on GitHub". Turning it on names the branch and says what it shows.
- When on, after each closed step and each finished task, Faber rebuilds BOARD.md from the project's plans, task files
  and recent commits (doing, checking, next, waiting, done; ten rows a list) and pushes it with the person's own git
  sign-in to the branch `faber-board`. No other branch is touched; a failed push is shown once and never stops work.
- The rendering follows `scripts/tasks.mjs board`, written in the engine.

Done: engine tests that fail today: the board text stays bounded whatever the backlog; publishing writes one parentless
commit to `faber-board` only; off means no git call. Extends: the project settings rows of the product map. What's-new
entry in every language. Coherence review. Two security reviews (it pushes to the person's repository).

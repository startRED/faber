# Keep follow-ups found at close as task files in .faber/tasks and offer them on the done card

Area: Agent's plan and closing box
Files: engine/src/progress.rs, engine/src/model.rs, engine/src/project/memory.rs, engine/src/engine/plan.rs, ui/Thread.tsx, ui/running.ts, ui/locales/, docs/design/
Depends: none
Size: about 350 changed lines

Why: `docs/superpowers/specs/2026-10-30-faber-method-2.md`, "Files and their limits", which replaces R435's `next.md`. A
person's project keeps its backlog the way Faber's own repository does: one file per task, so a long project keeps a real
list, two workers never edit the same line, and "ready" is computed from `Depends`.

Build:

- A `follow_ups` list on the `wrap_up` input. The engine writes each one as `.faber/tasks/<order>-<name>.md` through the
  no-follow write path, in the format of `docs/tasks/README.md`: a `# Title` line, then `Area`, `Files`, `Depends`,
  `Size` when known, and what done looks like. Each file is at most 6 KB; a longer one is refused with a plain notice,
  never cut.
- The done card (`doneCards` in `ui/running.ts` and its view in `ui/Thread.tsx`) lists the ready follow-ups (no unmet
  `Depends`), at most five with a count of the rest, with one button each, "Do this next", which sends the task's title
  and file as the next message. The file goes when its work lands (the conversation's closing box says done) or when the
  person dismisses it.
- If the project adopts its own task folder (task adoption-in-the-card), the files go there instead and `.faber/tasks/`
  is not used for that project.

Leave out: standing goals, and claims or lanes (a Faber-repo practice, never imposed on other projects).

Done: engine tests that fail today write a follow-up as a task file, refuse one over 6 KB, compute ready from `Depends`
and keep the files out of the start card; a screen check shows the done card offering a follow-up and removing it after
it is sent. Extends: the product-map rows "Agent's plan and closing box" and "Keep going"; the design page for the done
card. What's-new entry in every language. One review by the other vendor (R146), plus the coherence review of a change a
person sees.

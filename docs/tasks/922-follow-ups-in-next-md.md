# Keep follow-ups found at close in next.md and offer them on the done card

Area: Agent's plan and closing box
Files: engine/src/progress.rs, engine/src/model.rs, engine/src/project/memory.rs, engine/src/engine/plan.rs, ui/Thread.tsx, ui/running.ts, ui/locales/, docs/design/
Depends: none
Size: about 350 changed lines

Why: slice 3 of `docs/superpowers/specs/2026-10-22-own-work-method.md`, "Tasks and tool kits". For a person's project
`.faber/next.md` is the task list: follow-ups found and not done, one line each, at most 200 characters with why, 12 lines
at most. The done card offers them, and a line goes when its work lands. They are never loaded into the start card.

Build:

- A `follow_ups` list on the `wrap_up` input (each line bounded, at most 12 kept in the file; the oldest line is
  refused with a plain notice rather than silently dropped), appended by the engine to `.faber/next.md` through the
  no-follow write path.
- The done card (`doneCards` in `ui/running.ts` and its view in `ui/Thread.tsx`) lists the follow-ups with one button
  each, "Do this next", which sends the line as the next message of the conversation; sending removes the line when its
  work lands (the conversation's closing box says done) or when the person dismisses it.
- If the project adopts its own task folder, `next.md` is not used for that project (task adoption-in-the-card); until
  then `next.md` is the only list.

Leave out: standing goals (they take lines in a later step), and claims or lanes (a Faber-repo practice, never imposed
on other projects).

Done: engine tests that fail today bound a line to 200 characters and the file to 12 lines and keep a line out of the
card; a screen check shows the done card offering a follow-up and removing it after it is sent. Extends: the product-map
rows "Agent's plan and closing box" and "Keep going"; the design page for the done card. What's-new entry in every
language. One review by the other vendor (R146), plus the coherence review of a change a person sees.

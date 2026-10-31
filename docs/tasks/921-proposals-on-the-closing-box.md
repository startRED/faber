# Offer a taste line or a lesson on the closing box

Area: Agent's plan and closing box
Files: engine/src/progress.rs, engine/src/model.rs, engine/src/engine/plan.rs, ui/Thread.tsx, ui/Plan.tsx, ui/api.ts, ui/locales/, docs/design/
Depends: taste-lesson-records
Size: about 400 changed lines

Why: slice 3 of `docs/superpowers/specs/2026-10-22-own-work-method.md`. The agent proposes a taste line when the person
corrects the same kind of thing a second time, or once with "always" or "never"; it proposes a lesson at close. Both
show on the closing box (`WrapUp` in `engine/src/model.rs`, the `wrap_up` tool in `engine/src/progress.rs`), and the
person's yes makes the engine write them (task taste-lesson-records).

Build:

- Two optional fields of the `wrap_up` input and of `WrapUp`: `taste` (one line) and `lesson` (one line), each with a
  length limit enforced where the input is parsed (`progress::wrap_up`), and a sentence in `WRAP_UP_DESCRIPTION` saying
  when to use them (the two taste triggers above, a lesson only when it changes later work).
- The closing box shows each as a proposal with "Keep" and "No thanks", in the person's language, without a new surface
  (the box's existing question and button row). Keep calls the engine function of task taste-lesson-records; nothing is
  written before it.
- Not carried to Keep going: a proposal never counts as "done" and never blocks it.

Leave out: follow-ups and the done card (task follow-ups-as-task-files), and loading taste into the card beyond what
task start-card-in-sessions already reads.

Done: engine tests that fail today parse and bound both fields; a screen check shows the proposal, Keep writing the file
with its record and "No thanks" writing nothing; walk it at 720 px and in the longest language. Extends: the
product-map row "Agent's plan and closing box" and the design page for the closing box. What's-new entry in every
language. The commit body states the token cost of the added description text. One review by the other vendor (R146),
plus the coherence review of a change a person sees.

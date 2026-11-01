# Load a project's own decisions and task files into the start card

Area: Project memory
Files: engine/src/store.rs, engine/src/project/memory.rs, engine/src/project/rules.rs, engine/src/engine.rs, ui/ProjectMemory.tsx, ui/locales/
Depends: start-card-in-sessions
Size: about 400 changed lines

Why: the spec's "Adopt before create" (`docs/superpowers/specs/2026-10-22-own-work-method.md`): when a project keeps
decisions, taste or tasks its own way (Faber's own `docs/decisions.md`, a style guide, `docs/tasks/`), Faber uses that
place and never makes a second copy in `.faber/`. The data-folder component is built and read-only
(`faber-trust/adoptions.json`; `Store` `adopted_files_` and `adopted_task_approvals_`; `memory::read_adopted_source` and
`read_adopted_tasks`, each opened through no-follow handles with a 512 KiB cap), but the product map calls it "not yet
connected to the screen or start card". Check open pull request #436 (it exposes protected adopted project sources in
the engine) first: take what it landed, never redo it.

Build:

- Engine wiring: the pointer and the person's approval are set from Memory (a short "Use this file for decisions"
  action on a candidate the project already has), stored in the data folder only; a repository file can never supply it.
- The card (`rules::card`) loads adopted sources by the same byte shares as `.faber/` items; adopted content reaches
  the system prompt only when each item has a matching person record, like any kept decision.
- One-time reconciliation: when adoption starts and `.faber/` already holds items of that kind, list them once for the
  person, to move or keep, before anything is written. Until the adapters task lands the adopted kind is loaded and
  never written; proposals for it show only on the closing box.
- Containment is checked at every open on the opened handle, as the component does today.

Leave out: the adapters that write to adopted files (task adapters-for-faber-own-files).

Done: engine tests that fail today show a pointer outside the project refused, a link swapped in after approval
refused, an adopted decision loaded into the card within its share, and the reconciliation listed once; a screen check
for the Memory action. Extends: the product-map rows "`faber-trust/adoptions.json`" and "Project memory", and the design
page for Memory. What's-new entry in every language. Two reviews, one by the other vendor, because this is security
(R146).

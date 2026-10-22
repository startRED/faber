# Faber's own work method for every project

Take this task only when it is the last free one in the queue (the owner asked for it after everything else). If
another task is free, run `node scripts/tasks.mjs release own-work-method` and take that one instead.

## The task

Faber is a workspace that gives agents tools, context and rules, so it should carry its own work method, the way BMAD
does for other setups, but built from what was learned while building Faber. Today that method exists only for
Faber's own development (AGENTS.md, the docs index with living and dated pages, the product map, the taste cards, the
R-numbered decisions, the task queue, lane claims, landing through the queue, cross-vendor review). For a person's
projects most of it is planned and not built (docs/product/building.md "Coherence of a project's work" and the brief in
five parts; docs/product/following.md "Learning and memory"; docs/superpowers/specs/2026-10-16-own-lessons-into-faber.md).

Redo it from scratch in the best way you can find: one method, written once, that Faber sets up and keeps by itself in a
project's `.faber/` folder (rules, project map from the code, decisions, taste, tasks, lessons), so the person writes
nothing (tool kits are assembled per task, never hand-written). Faber's own development then runs on the same method, so
there is one way, not two.

## Sources to fold in

- Everything in AGENTS.md, docs/product/, docs/dev/, docs/product-rules.md and docs/decisions.md, and the maintainer's
  notes and lessons.
- The owner's reading: Felix Rieseberg, "How to be more ambitious: a tutorial for humans with models"
  (https://felixrieseberg.com/how-to-be-more-ambitious-a-tutorial-for-humans-with-models/). Its ideas, in short: aim at
  the goal behind the task (outcomes, not single fixes); build loops that run and verify without the person; give raw
  data and real access instead of templates; ask for decisions with options, not reports; question the process and test
  a new one against past history; ask what a top expert would do and what independent evidence would confirm it; set
  hard, specific targets; test assumptions against data; never let a model grade its own work, use outside checks it can
  run; bring in other fields' methods (a red team, a forensic view) by doing them, not by role-play.
- How other methods (BMAD and similar) organise roles and documents, studied for ideas only: no code, files or
  personas copied, nothing third-party bundled.

## Done looks like

A spec in docs/superpowers/specs with a `Status:` line and a row in docs/README.md, approved by the owner before any
build: what the method is, which files Faber keeps in `.faber/` and how each stays small and current, how it maps onto
what already exists (Extends: each map entry), what it replaces, its memory and token cost, and the first build slices
as new tasks in this queue.

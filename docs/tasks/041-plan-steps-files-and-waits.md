# Plan steps name the files they expect to touch and the steps they wait on

Area: Agent's plan and closing box
Files: engine/src/model.rs, engine/src/progress.rs, engine/src/engine/plan.rs, engine/src/persona.rs, ui/Plan.tsx, docs/product-map.md
Depends: none
Size: about 300 changed lines

Why: slice 2 of `docs/superpowers/specs/2026-10-30-faber-method-2.md` (approved by the owner, R484 of the commit that
queued this task). Waves need a work graph: today a plan step is text and a state, with no id, no files and no order
besides the list (`engine/src/progress.rs`, at most 12 steps).

Build:

- Each step gains a short stable `id`, optional `files` (paths or folders it expects to change, at most 20, each at most
  200 characters, relative to the project) and optional `waits` (ids of steps it needs first). Old plans without them
  still load and behave as today.
- The engine validates a plan when the agent writes it: unknown ids, a step waiting on itself and cycles are refused with
  an `error.plan.*` key the agent reads, never silently dropped.
- The leader's persona text (`persona::LEADS`) asks for `files` and `waits` on plans that hand work out, in one sentence.
- `ready(plan)`: the steps whose waits are all done; tested here, used by the wave task.

Leave out: starting anything (task 042), the overlap check after the work (task 043), any new screen; the plan view only
keeps working.

Done: engine tests that fail today: a plan with ids, files and waits round-trips; a cycle and an unknown wait are refused
with their keys; an old plan without the fields loads; `ready` follows the waits. Extends: the product-map row "Agent's
plan and closing box". No what's-new entry (nothing a person sees yet). One review by the other vendor (R146).

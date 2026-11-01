# Before the leader joins a wave, Faber compares what each worker really changed and checks the combined result

Area: Conversation started by the agent
Files: engine/src/engine/plan.rs, engine/src/engine/copies.rs, engine/src/engine/siblings.rs, engine/src/engine/verifying.rs, engine/tests/, docs/product-map.md
Depends: waves-start-workers-from-the-leaders-commit
Size: about 400 changed lines

Why: slice 2 of `docs/superpowers/specs/2026-10-30-faber-method-2.md`, step 4. Declared files only schedule: two workers
can both find they must change a shared export or a lockfile. Faber never merges (R201); the leader brings the work in.
Faber decides what is safe to bring in and whether the result holds.

Build:

- When every step of a wave has finished, the engine lists each worker's real changes against its base commit (added,
  deleted and renamed files included) with the copy runner's fixed git calls.
- Two workers whose real changes overlap without both declaring it: the later step is marked to run again on top of the
  joined result, and the leader is told which step and why, in one line.
- The leader merges the others into its own copy (the integration copy); then the engine runs the project's checks
  (`verify`, fast level) on the combined result.
- A red check that names files of one step goes back to that step's worker, on the combined version, once; a failure with
  no clear owner, or a second failure, goes to the leader.

Leave out: any merge done by Faber itself, and conflict resolution by the engine.

Done: engine tests on scratch repositories that fail today: an undeclared overlap reruns the later step instead of
merging both; a clean wave reaches the combined check; a red check routes to the step whose files it names, then to the
leader on a second failure. Extends: "Conversation started by the agent". Two reviews (it decides which work is kept).

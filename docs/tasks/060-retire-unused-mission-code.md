# Retire the mission code and the local build

Since R171 and R202 (self-development through the project conversation), missions have no screen and no way to start.
The local build of R93 (slots A and B, the canary, probation and rollback) only ever started from a landed mission and
its canary is a mission, so it goes too (R215). Remove the parts of
`engine/src/project/` (coordinator, lead, worker, queue, kits, qualification, bar, run, canary, update) and of the shell
(`src-tauri/src/local_update.rs`, the `faber-local` program, `mission_*`, `project_proof*`, `project_grant` and what
only they use) that nothing else reaches, with their tests, testkit scenarios, fixtures, records and product-map rows.
Keep what the project conversation uses (copies, checks, git, remote) and the public updater of R156. Old
`.faber/missions/` journals in people's projects and an old development data folder stay readable or are ignored,
never deleted.

Done when: the engine and shell build without the removed modules, every remaining test passes, the product map's
mission and local build rows and the "Known incoherences" about them are gone or updated, R93 is marked retired in
decisions.md, and the commit body gives the lines removed and the memory and binary-size change.

Progress: wave 1 (the shell, the screen, the `faber-local` program and its testkit scenarios) is done. Left for the
engine wave: `testkit/examples/project-qualification` (its `--live-promote` mode starts the removed `faber-local`), the
`projects.rs` screen scenarios and their fixture probe, `reason.kit.*` keys (still emitted by `project/kit`), the unused
`projects.task.review` and `projects.state.*` keys, the `mission_start` trap in `testkit/coherence/traps.json`, and
renaming `Engine::local_build_admission` (the updater's install gate) once the mission term is gone.

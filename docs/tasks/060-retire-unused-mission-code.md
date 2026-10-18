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

Progress: wave 1 (the shell, the screen, the `faber-local` program and its testkit scenarios) and wave 2a (qualify,
the proof screen, the r132 harness, the qualification examples, the coordinator and mission screen scenarios) are done.
Left: the mission cluster in one wave (`run`, `store`, `coordinator`, `lead`, `queue`, `worker`, `update`,
`canary`, `report`, `bar`, `workspace`, `proof/native` and the `engine/` code that calls them:
`project_sessions.rs`, `project_budget.rs`, `Project::has_active_mission`, with their tests), then `policy`,
`qualification`, `kit`, `disk`, `files` and `wire` split from what the project conversation still uses. Also:
`reason.kit.*` keys (still emitted by `project/kit`), the `mission_start` trap in `testkit/coherence/traps.json`,
the screen suite's `rustup run` wrapper in ci.yml (it was for the canary's cargo), and renaming
`Engine::local_build_admission` (the updater's install gate) once the mission term is gone.

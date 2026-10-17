# Retire the mission code nothing reaches

Since R171 and R202 (self-development through the project conversation), missions have no screen and no way to start.
Find which parts of `engine/src/project/` (coordinator, lead, worker, queue, kits, qualification, bar) and which shell
commands (`mission_*`, `project_proof*`, `project_grant`) nothing reaches once task 050 no longer needs them, and remove
them with their tests, records and product-map rows, keeping what the project conversation uses (copies, checks, git,
remote). Old `.faber/missions/` journals in people's projects stay readable or are ignored, never deleted.

Done when: the engine and shell build without the removed modules, every remaining test passes, the product map's
mission rows and "Known incoherences" about missions are gone or updated, and the commit body gives the lines removed
and the memory and binary-size change.

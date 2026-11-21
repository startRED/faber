# Verify stopped-helper recovery

Area: project
Files: `engine/src/engine/helpers.rs`, caller completion, isolated live proof, product map and design
Depends: none
Size: one pull request

Stopped and failed helpers now report a bounded reason and wake their caller; tests cover helper completion and preservation of other workers. The historical 137.743-second stop has no recorded actor or cause.

An isolated live Windows run stopped a Terra low read helper and confirmed that its lead resumed once on the
same process. The lead received confirmed writer exit with unknown CLI cause, read the real merge conflict and
the absence of registered helpers and running commands, then returned its receipt. See the
[live run](../dev/autonomy.md). Other stop paths and free-form claims in ordinary model replies were outside this run.

Done: In an isolated folder, stop a cheap-model helper mid-task and verify its caller wakes with the observed reason and relevant folder state, then recovers without a person. Prove the live-work check and stop-cause visibility. Add regressions for missing behavior. Do not read or change active conversations or the person's data.

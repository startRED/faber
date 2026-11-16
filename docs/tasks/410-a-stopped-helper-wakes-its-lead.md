# Verify stopped-helper recovery

Area: project
Files: `engine/src/engine/helpers.rs`, caller completion, isolated live proof, product map and design
Depends: none
Size: one pull request

Stopped and failed helpers now report a bounded reason and wake their caller; tests cover helper completion and preservation of other workers. The historical 137.743-second stop has no recorded actor or cause. The requested real isolated run stopping a helper mid-task and proving recovery has not been established, nor has reporting the folder state for an interrupted rebase or merge.

Remaining: report folder state (clean, rebase/merge in progress, conflicted files); verify and add missing enforcement before a lead claims work is running when no live helper or command remains. Preserve the observed stop initiator/cause when available and label unknown causes explicitly; never invent the historical actor.

Done: In an isolated folder, stop a cheap-model helper mid-task and verify its caller wakes with the observed reason and relevant folder state, then recovers without a person. Prove the live-work check and stop-cause visibility. Add regressions for missing behavior. Do not read or change active conversations or the person's data.

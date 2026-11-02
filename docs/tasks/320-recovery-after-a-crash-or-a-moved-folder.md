# Recovery after a crash: the hard-kill screen proof

Area: Projects, Shell, recovery
Files: testkit/src/screen/killed.rs, testkit/src/screen/mod.rs, testkit/src/desktop.rs, testkit/screen-costs.json, tests/ui/, docs
Depends: none
Size: one pull request

Why: The missing-folder note and the unclean-exit note landed, but no screen test proves that a saved answer and the native session survive a hard kill together and that the interrupted turn then shows as interrupted and resumes.

Build: A screen check starts the app, sends a message to the fake CLI, kills the app process mid-answer, restarts it and proves the saved answer is still there, the turn shows interrupted with Continue offered, and Continue resumes the native session. Put the scenario in `testkit/src/screen/killed.rs`; record its cost in `testkit/screen-costs.json`.

Leave out: new recovery behaviour, new strings and any change to how a crash is told.

Done: The check fails when the saved answer, the interrupted state or the resume is missing, and passes now. Report the added screen time and keep the suite inside its budget.

# Screen check: the app is killed in the middle of an answer; on reopening the answer is stopped and the next message goes on

Area: Test bench (recovery)
Files: testkit/src/screen/killed.rs, testkit/src/screen/mod.rs, testkit/src/desktop.rs, testkit/screen-costs.json, docs/dev/testing.md
Depends: none
Size: about 300 changed lines

Why: engine tests cover a reply cut by a crash, and "real work 3" reopens a closed app, but no screen scenario ends the app's process while a reply runs (`agent_history` only closes it). The window, the saved file and the CLI resume have never been proven together after a hard kill.

Build: one scenario with the fake CLI for each vendor: start a long streaming answer, end the app with `Desktop::kill`, launch again from the same data folder and check the screen. Then send a second message and have the fake CLI record whether it was started on the same native session. No product code unless the scenario fails. Use a new file: `testkit/src/screen/app.rs` and `real_work.rs` belong to another lane.

Done: the scenario fails if the old answer still shows as typing or is missing, if the text streamed before the kill is lost, if a CLI process from the first run is left, or if the second message starts a new session. Both vendors, memory under the 400 MB limit, its cost listed in `screen-costs.json`.

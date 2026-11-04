# The update restart must complete while a conversation is working

Area: Updates
Files: update/src/updater.rs, sidecar/src/main.rs, electron/, tests
Depends: none
Size: about 100 changed lines

Why: the 1.0 gate's Electron preview.89 to preview.90 update proof (CI run 41800600209, workflow `install-update-self.yml`, evidence in reports/reforma/helpers/g390i.result.md) failed: with a conversation open and working, the real "Update ready, restart" button leaves the old Electron main and renderer processes alive after the sidecar exits; `update.log` says the installer was waiting ("Faber was still closing"), and the installed package stays at .89. A reviewer read the .89 sources: `Live::end` in `update/src/updater.rs` queues a quit order, blocks its Tokio worker with `thread::sleep(QUIT_GRACE)` and then `process::exit`, while `sidecar/src/main.rs` needs a separate task (the stdout writer) to flush that order. With Tokio's non-stealable LIFO slot the wake can sit behind the sleeping worker, so the order never leaves. The same code is on the base today. This is the "1.0 updates itself to 1.0.1 while a conversation is open" item of docs/dev/release-1.0.md.

Build: first reproduce with the workflow's Electron case (or a unit test with a current-thread runtime that shows the order never flushes). Make the quit order leave before the process ends: yield and wait for the writer's flush (an acknowledgement or a bounded wait on a flush signal) instead of sleeping the worker, and make sure the shell closes its windows and renderers on quit. A test fails without it. Then run the install-update-self workflow's Electron case on a preview that has the fix and cite the run.

Leave out: the updater's download and signature checks.

Done: the actual restart button replaces the running Faber with the new version while a conversation is working, the conversation continues, and the check fails without the change. An older installed build still has the race, so state in the release notes how people on it update (a second restart or a manual install).

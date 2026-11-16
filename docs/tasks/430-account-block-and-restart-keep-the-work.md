# Complete native-shell restart and account recovery proofs

Area: project
Files: isolated native shell proofs in `electron/`, `src-tauri/` and `testkit/`; product map and design
Depends: none
Size: one pull request

Delivered: persisted waits, startup deadline restoration, receipt deduplication and release on real account/quota confirmation. Controlled tests and the isolated live Haiku engine-recreation probe passed. An isolated native Electron process restart also preserved the exact deadline and reached visible refusal without an external event or model turn (128.12 MB no-CLI). Do not reimplement account confirmation or finite timers.

Remaining: prove the full native shell lifecycle, including deadlines that expire while the app is closed and account refusal released after restart, on the shipping shell. Engine recreation alone is not a native-shell restart. Read [the diagnosis](../dev/autonomy.md) for additional native proof completed since this task was written.

Done: A bounded isolated native run demonstrates those paths without lost work or duplicate paid sends, with the cheapest permitted model where needed. Record process/memory cost. Never touch the installed app in use, active conversations or CLI credential files.

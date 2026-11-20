# Complete native-shell restart and account recovery proofs

Area: project
Files: isolated native shell proofs in `electron/`, `src-tauri/` and `testkit/`; product map and design
Depends: none
Size: one pull request

Delivered: persisted waits, startup deadline restoration, receipt deduplication and release on real account/quota confirmation. Controlled tests and the isolated live Haiku engine-recreation probe passed. An isolated native Electron process restart also preserved the exact deadline and reached visible refusal without an external event or model turn (128.12 MB no-CLI). Do not reimplement account confirmation or finite timers.

Native proof: a hidden Windows Tauri test closed before the deadline, reopened after it expired, kept a visible account refusal, resumed once after a synthetic account check and reopened a third time without another model invocation. The disconnected process tree used 159.9 MB. See [the diagnosis](../dev/autonomy.md).

An isolated native Tauri run with an overdue wait, real subscription confirmation and GPT-6 Luna/medium received one
reply and preserved it after reopening without another send. The live process-tree CPU and memory samples are in
[the diagnosis](../dev/autonomy.md).

A hidden Electron run on Windows used the same overdue-wait scenario. The first process kept the refused account,
both saved messages and the exact deadline. The second confirmed the real subscription, with paid fallback unavailable,
and returned one GPT-6.1 Terra/low answer. The third reopened that answer and confirmed the account again.
The gate recorded one eligible inference request, and all three owned trees exited. The dry run had one eligible turn
and forwarded no inference. The finished reply and earlier work were visible at 1280 and 390 pixels.

The live run took 14.38 s. Before the account was available, four live processes held 138.5 MB private working set
and 2.72 accumulated CPU seconds. At completion, fourteen held 593.3 MB and 8.33 CPU seconds. These samples include
the account and inference gates. The reply reported 30,244 input and 13 output tokens (USD 0.030309 API equivalent).
Peak memory and whole-run CPU were outside the measurement.

Done: A bounded isolated native run demonstrates those paths without lost work or duplicate paid sends, with the cheapest permitted model where needed. Record process/memory cost. Never touch the installed app in use, active conversations or CLI credential files.

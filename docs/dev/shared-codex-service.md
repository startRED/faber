# Shared Codex service recovery

This page records the controlled Windows recovery exercises and the separate resource measurements.

The three-conversation regression test holds an approval in one conversation while two others go silent. The
old driver sent a health probe while the approval was pending. With the protection in place, the approval remains
deliverable and the health probe can run after a fresh quiet window once the person answers. The test uses only
an in-memory protocol fixture with a paused clock.

Five starts of stock Codex 0.166.0 with fresh CLI profiles on Windows answered the startup handshake in 195–274 ms.
Those samples started no turns and used no subscription. Faber allows 30 seconds. In a controlled run with no handshake
answer, two waiting conversations sent no turn and each received the failure after writer exit was confirmed.

An isolated Windows exercise used the stock Codex 0.166.0 server, a fresh profile and a loopback model stream. Two runs
held three turns at once. The process tree used 99 MiB idle and 123–124 MiB while held (rounded up), with 76–81 threads.
The local `thread/loaded/list` probe answered in 1.21–3.62 ms. A third run held a native `webSearch` open and interrupted
it using the [turn-specific control](https://learn.chatgpt.com/docs/app-server#interrupt-a-turn). The other two turns
completed, with three total loopback requests. The held tree used about 123 MiB and 95 threads in that run. A separate
exercise ran the Faber engine against the stock server with the same local model stream and an advanced clock. The engine
started one server for three conversations, stopped the lookup through its own silence watch, and both peers completed.
The subsequent Keep going retry is covered by the engine test.
For the single-reply MCP check, the stock 0.166.0 server reported a native tool in progress while a local fixture held
its result. The same `thread/loaded/list` call answered in 3 ms during that hold. No provider request was made by the
check. Once released, the tool and reply completed, and both fixture workers exited when the server closed. The driver
tests cover the 15-minute boundary, recovery when the wire never responds, and a healthy call that stops qualifying once
it completes. The stock test used a loopback provider and neither a subscription nor an installed Faber conversation.
The stock command exercise reached a policy refusal at process creation in the fresh read-only profile for both
PowerShell and `cmd.exe`. It does not establish command recovery in that profile. A recorded session confirms that
native command starts report an in-progress status and the owning turn ID. A separate integration test fed that
event to Faber's driver with an owned Windows process tree. After the quiet boundary and unanswered check, it confirmed
the root and two workers had exited before releasing either reply. The case without a verifiable process tree
reported the stop as unconfirmed instead.
Closing the last conversation now lets any opening already queued on that service reach it before idle shutdown.
A queued tools update alone still lets the service stop after the last conversation releases its writers.
The Windows loopback run used one server. It reused the first three conversations for two waves, then closed and
replaced all three before each of eight more waves. All 30 expected replies arrived, and all 27 conversations'
writer exits were confirmed when the service stopped at the end.
A request that reaches an already closed service reports the error through the driver runtime. This keeps the
error callback from trying to take the engine lock again on the same call stack.
These samples establish short-run behavior in this setup; service growth under a long real workload is still unmeasured.

A follow-up on Windows completed 240 loopback replies on one stock server in 80 waves of three. Two conversations were
closed and replaced in each of the last 30 waves. The server process went from 40.7 to 162.7 MiB private memory, 119.8 to
244.1 MiB resident memory and 417 to 582 handles. The last sample recorded 191.1 CPU seconds. These are process-level
samples from the synthetic run. They leave the cause of growth in a real project open.

A longer Windows loopback run with the stock 0.166.0 server completed 390 replies in 130 waves of three. It reused
the same three conversations for 90 waves, then replaced two per wave for 40 waves. It confirmed 83 writer exits and
the process tree was empty at the end. At the last in-run sample (285.4 seconds), the server had grown from 47.1 to
195.7 MiB private memory and from 121.7 to 273.5 MiB resident memory. It had used 352.2 CPU seconds and still had nine
descendants at that sample. This loopback run establishes bounded test completion and cleanup under churn. A real
project's sustained growth and a separate production resource budget remain open.

A comparison on the same Windows build kept all three conversations for 130 waves and completed the same 390 replies.
At the last in-run sample (102.4 seconds), private memory had gone from 51.9 to 105.6 MiB, resident memory from 127.1 to
184.9 MiB, and handles from 471 to 482. CPU time was 119.1 seconds. The only descendant at that sample was the console
host. All three writer exits were confirmed at shutdown, with an empty process tree. The stable conversations had longer
final requests than the two replaced conversations in the churn run. This pair points toward investigating resources
retained across replacements. It does not isolate the cost of individual descendants, establish a leak or reproduce the
production stall.

Two shorter runs each completed 60 replies through the Faber driver. Replacing the hidden-console launch with
[a detached launch](https://learn.microsoft.com/en-us/windows/desktop/ProcThread/process-creation-flags) removed the
console host in that experiment. Other CLI descendants continued to occupy the job, and neither run confirmed any
individual writer release while the peer stayed open. The normal launch remains in place. The experiment indicates
that removing the console host alone is insufficient to release the copies safely.

On 2026-11-19, a read-only Windows process snapshot found 148 direct descendants of one live Codex service owned by
Faber, with about 9.6 GiB of combined private memory. Its 72 Node processes held 9,478 MiB of that total. This is a
single snapshot, without per-conversation attribution. A [public report for the desktop client](https://github.com/openai/codex/issues/35485)
describes a similar worker buildup. No processes from the live service were stopped for this measurement.

A stock 0.166.0 service with a fresh local-only profile and a synthetic MCP worker per conversation confirmed that
`thread/unsubscribe` allowed one idle runtime and its worker to unload while a second conversation kept its worker.
The same operation proposed only after Faber confirmed writer exit did not help in the 130-wave loopback run. It
completed 390 replies, sent zero unsubscribe requests while the service was live and timed out waiting for unload.
Other members in the shared Windows process tree held the existing writer barrier. Any change that requests native
unload earlier must keep the barrier and account for [shutdown hooks](https://github.com/openai/codex/blob/rust-v0.166.0/codex-rs/core/src/hook_runtime.rs#L434)
before releasing a copy.

With explicit close requesting unload once known work drained, the same 130-wave loopback completed 390 replies
and unloaded all 80 replaced runtimes. At reply 390, 60 had unloaded and the service held 122.5 MiB private and
203.4 MiB resident. After all 80 unloaded, it held 93.2 MiB private and 174.8 MiB resident. The earlier attempt
held 196.0 MiB private and 275.9 MiB resident at reply 390 and never observed an unload. The shared job still held
17 descendants after runtime unload, so Faber kept its writer leases. At final service shutdown it confirmed all
83 writer exits and the owned tree was empty. The measurements cover one loopback run per design, not installed
projects or the memory use of the Faber process itself.

A separate loopback with one synthetic Node MCP worker per thread completed 24 replies through Faber's driver.
Three workers became 13 after ten replacements, then returned to three when the old runtimes unloaded. The continuing
conversation retained its original worker. All writers exited when the service finally stopped. The stock service
also honored a two-second idle grace requested on each opened thread: one worker exited and its peer stayed alive.
With that grace set by Faber on identified conversations, another loopback finished 48 replies and unloaded all
eight replaced runtimes. The worker count dropped from seven to three after the last wave, and the ongoing
conversation kept its original worker. At shutdown Faber confirmed 11 writer exits and the job was empty.

Three fresh Windows runs of stock 0.166.0, with no hook, model turn or personal tool configuration, sampled the
whole live tree at three points. With no conversations it held 115.9–116.1 MiB resident across 59–62 native
threads and 0.22–0.27 accumulated CPU seconds. With two open, it held 167.2–169.0 MiB resident, 92–95 threads
and 0.67–0.78 CPU seconds. With 32 open, it held 155.4–210.7 MiB resident, 70–95 threads and 1.48–2.03 CPU
seconds. These are snapshots with scheduler variance. Resident sums can count shared pages repeatedly, and
the CPU sums exclude descendants that had already exited.

A separate read-only snapshot on 2026-11-20 at 04:47 UTC counted 406 processes under a live service from
the installed Faber. Private commit totaled 15.76 GiB. The 120 Node processes held 13,415.6 MiB of it.
Eighty were direct children of Codex and 40 were children of command shells spawned by Codex. Node processes
first appeared on November 18, in groups of three or multiples of three per sampled birth hour. This indicates
retention across a long-running service. The snapshot does not identify each worker's tool or conversation, or
show that every worker should have exited. No command lines were read and the service was left running.

The monitor observed 2.7 GB/537 threads with nearly unchanged CPU and silent conversations, followed by
2.53 GB/467 threads and estimated growth of 70-280 MB per 30 minutes; manual restarts restored progress.
These are monitor observations, not measurements of an internal leak by this lane. They do not justify
blindly killing a service based on memory alone. On the supported platforms the existing budget queues a new
automatic CLI admission if combined owned private working sets reach half of physical RAM or if available RAM
falls below 2 GiB. It leaves existing work running. This budget is separate from the no-CLI R125 app check
and measures private working sets rather than the private commit measured in the read-only snapshot.

## Automatic idle release

On 0.166.0 and later, Faber can passively unsubscribe a saved conversation after two idle minutes. Both the engine and
the driver must agree that work has drained. The driver checks the native goal, and retries one failed check. A new
turn invalidates an earlier check. Temporary threads, outstanding goals, threads that called an MCP tool, and threads
with possible leftover children after local commands or hooks stay loaded. Servers with user hooks explicitly enabled
by Faber also stay loaded. The next message opens the same saved thread.

If the close notice is lost, Faber checks once more automatically and again on demand. If only retired threads remain
and a fresh check proves the owned process tree has no descendants, the service can stop. An uncertain release with
remaining descendants keeps its writer lease. If a protected resume runs out of time before even opening the thread,
the existing saved-transcript fallback can send the new turn once. The idle timer runs in Faber's process and is
cancelled by new work.

A Windows control on stock 0.166.0 shortened the engine timer to four seconds and used a loopback model and one
fixture MCP worker per runtime. The shared service and its workers went from two workers and 218.2 MiB resident to
one worker and 164.2 MiB as the first thread unloaded. The peer kept its original worker, and the next send opened
the same saved thread with a new worker. Exactly three model requests produced three replies. Both threads then
unloaded and no fixture workers remained. One other owned process remained with the shared service until the test
engine dropped it, at which point the owned tree was empty. The numbers are snapshots of the service and fixture
workers and do not measure other integrations.

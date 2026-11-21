# Autonomy checks and remaining work

The finite-wait repair is on main as `a7e3ee8bd5d62cf8b9f4ec84869b3b6c230009f6`.
Its [full remote verification](https://github.com/startRED/faber/actions/runs/43828218007) passed.
This page distinguishes the delivered behavior from the older installed app and remaining work.

## Confirmed defects and repair

Declared waits previously depended on an event without a finite fallback. They now persist a deadline;
startup restores it without resetting the clock. At the deadline the engine checks actual followed workers,
requests one continuation or saves a visible admission/delivery blocker. Uncertain delivery is not retried.
A save that fails while restoring a wait, moving its deadline or reserving its return gets one local retry after
15 seconds. The retry keeps the original deadline and checks that the wait is still current. Once the engine
attempts to submit the return, the existing receipt and uncertainty rules apply.
A persistent storage error remains visible. Only a later successful save of that conversation or a restart that
can save it again clears this specific pre-dispatch blocker and restores the deadline.
PR waits retain their head/state/push baseline, so passing checks on a new commit of the same PR and an
already merged PR return the waiter; stale readings do not. Healthy caller completion no longer cancels
actual helpers. Terminal helper reports preserve a bounded reason and partial output before delivery.
Bounded acknowledgements and checked writes prevent late control writes into a torn stream.

The local supervisor command uses the same saved wait and receipt. `faber wake` checks the exact observed deadline,
reports a stale or blocked request, and records the local channel as the source of an accepted return. An unanswered
permission holds a return on every trigger and while it is in the queue. A plan approval goes ahead of the queued
return. A supervisor process must already be running on an awake computer.
Subscription confirmation and quota are separate saved reasons. Fresh evidence can update the reason without admitting a turn.

A background execution can end after its assistant reply. Faber retains up to 32 recent endings in memory so a resumed
project agent can find the execution and inspect masked capture. A late event updates only its originating result.
The raw capture stays out of the saved conversation, including the echo of the internal read tool. An explicit CLI call through a shell can show
its masked output in the caller's ordinary command log. Capture adds no
process or timer and bounds output to 256 KiB across the app plus labels and metadata. A restart clears the cache.

When an interrupted conversation starts again, an accepted return at the head of its runnable queue can serve as the
restart turn without another model request. The engine keeps the saved return's identity and prompt. Its own
interruption needs an explicit continuation. Stop and a pending permission lost on restart leave the accepted
item queued for an explicit send. It no longer holds new messages once the person takes up the conversation.

The actor behind the historical 137.743-second helper interruption was not recorded. A normal 900-second
silence limit does not explain it. The reproduced caller-completion bug is fixed; it does not establish
who caused that historical interruption.

## Evidence and limits

### External Windows process proof

Build the desktop command and the windowless example with the project Cargo wrapper. Pass their executable paths to the
PowerShell 7 proof:

```powershell
node scripts/cargo.mjs build -p faber-cli
node scripts/cargo.mjs build -p faber-channel --features host --example external_wake_host
pwsh -NoProfile -File scripts/prove-external-wake.ps1 -CliPath <faber-cli.exe> -TestHostPath <external_wake_host.exe>
```

The example uses Faber's real command host and local channel with its own data. The proof checks `tooSoon` before the
saved deadline, closes the isolated host, and uses the same command to start a second process after the deadline. It
checks the persisted account blocker, an obsolete deadline, no extra reply, and the exit of both captured command
streams while the host stays up. The native timer and the explicit request can both reach the blocker, so the receipt
records the outcome and the process change. This uses zero model calls and requires an awake computer. It checks the
command and engine with the windowless host. The installed desktop and an operating-system schedule need separate proofs.

On 2026-11-19, the prior Windows command exited but left its captured streams open until the child host ended. The same
proof passed with detached launch and handle inheritance disabled. It observed two distinct host processes, the same
deadline before and after restart, the persisted account blocker and no new reply. Cleanup stopped both test processes.

### Other proof results

On Windows, Claude Code 2.1.312 offered the native PowerShell tool in a disposable print session. One background
`Write-Output` call reported a correlated `task_started` of type `local_bash` and a matching `completed` notification.
The driver replays both shells across a late start, a new reply and a held helper. This live sample checks the native
event type and IDs; the controlled driver checks cover reply ownership and the Stop route.

- Regression checks failed before the corresponding fixes. Final local proofs: 483 engine library cases,
  41 helper cases, 74 and 82 driver recordings, 13 focused UI cases and all seven project checks passed.
  The final 17 wait cases include no-event deadlines, restart receipts, account refusal, new heads and merges.
- On 2026-11-19 the isolated live Windows `autonomy_probe` completed in 42.7 seconds with Haiku/low.
  Process 24960 saved the conversation and exited without engine shutdown. Process 16020 read the same wait start and
  deadline from disk 28.633 seconds early, then observed the account blocker 9 milliseconds after the deadline. After
  account confirmation it recorded one resumed reply. Process 15444 observed the same reply with no wait or dispatch
  attempt after another account check. The two conversation sessions reported USD 0.006793 in API-equivalent cost,
  excluding account-control probes. The second engine process used 39.7 MB at the end. Peak memory for the process
  tree and the installed app were outside the proof. A real read of a historical merged GitHub PR requested a return
  with zero model processes permitted.
  The rerun after tightening the probe's admission stopped before its first conversation: the CLI confirmed the
  subscription but did not return a fresh control reading showing that paid fallback was off. The proof now
  requires both facts before applying the account event that can release the saved wait.
- An isolated native Electron process restart preserved the exact saved deadline. Its restored timer
  reached visible account refusal without an external event or model turn; no-CLI memory was 128.12 MB.
- A hidden Windows Tauri test closed the app before the saved deadline and reopened after it expired. The
  exact deadline and visible account refusal survived. The fake account confirmation caused one resumed model
  invocation, with the earlier messages included in its input. A third process reopened the saved reply and
  repeated the account check without dispatching again. The disconnected process tree used 159.9 MB.
- A separate hidden Windows Tauri run started with an overdue saved wait and an unavailable CLI. It preserved the
  deadline, the visible refusal and both messages. A second process confirmed the real subscription, with no paid
  fallback available, and received one GPT-6 Luna/medium reply. The third reopened that reply and repeated the
  account check without sending again. The 11.42 s run contained all three app trees. The disconnected sample
  held 127.2 MB private working set and 2.69 accumulated CPU seconds across seven live processes. At reply
  completion the sample held 603.1 MB and 9.28 CPU seconds across 17 live processes, including the test's CLI gate
  and account checks. The outer meter missed those processes: these are live-tree snapshots, not peaks or
  whole-run CPU totals. The 4.15 s reply reported 25,716 input and 13 output tokens (USD 0.002578 API equivalent).
- The native app was exercised with synthetic data and inspected at 1280, 720 and 390 pixels. The final
  no-CLI fixture measured 166.55 MB, below the 400 MB test ceiling. No resident production process was added.
  CLI process-tree peak and incremental processor cost were not measured.
- A 4.34-second Windows integration run terminated an owned helper process mid-task in a disposable real Git
  worktree. The helper had created a merge conflict. The engine retained the partial words, included confirmed
  writer exit and the unknown CLI stop cause, and started exactly one caller turn with no new person message.
  The caller's workspace action reported the merge, the conflicted file and zero registered helpers or reported
  commands. The scripted caller aborted the merge and checked the clean copy and unchanged source file.
  The earlier run failed because the fatal report dropped the writer confirmation and unknown cause. The test
  `stopped_helper_wakes_its_lead_with_a_real_merge_conflict` uses a real local worker with a scripted CLI protocol,
  with no model calls. Recovery by an actual cheap-model lead and helper remains unverified.

## Installed app and outstanding work

The monitor reported a missed wake for nine minutes after the revert's checks passed. That earlier saved
wait had already cleared when the person wrote, so its pre-message state could not be recovered. A subsequent
read-only inspection, immediately after this lane declared another wait at 08:09:49 UTC on November 16,
did observe only `note` and `since`, without `until`, in the installed app's own saved wait. These are
distinct observations. The isolated delivered code persists a finite deadline; the installed older app
was not updated or disturbed while real conversations were active.

- [400](../tasks/400-every-wait-has-a-wake.md): the person installs the update; then verify saved deadlines,
  autonomous return and truthful wait/blocked/completed UI with isolated data. Installation is not a source fix.
- [410](../tasks/410-a-stopped-helper-wakes-its-lead.md): an isolated Windows live run stopped one owned Codex
  read helper while its lead waited. The original lead process stayed live, received the confirmed writer exit
  and unknown CLI cause, and resumed once. Its workspace call observed one real merge conflict, zero registered
  helpers and no running command before the model returned the expected result. The harness then aborted its
  merge. The source had only the expected generated rules, identical to a no-inference comparison.
  The saved result arrived 18.17 seconds after the first send. The lead's two Terra low turns reported a
  combined USD 0.024088 API equivalent; usage for the interrupted helper was unavailable. The run used
  ephemeral threads in a disposable repository and the existing subscription. Earlier attempts failed in the
  harness. An offline cleanup check accounted for the generated rules. This is one live case.
- [420](../tasks/420-the-screen-tells-waiting-from-stuck.md): hidden Electron checks distinguished a healthy
  wait, a blocked return after a followed conversation disappeared, and a finished report waiting to reconnect.
  A blocked return now shows its reason and any saved note, without counting the old references as active waits.
  The recovered report persisted once. The account and draft actions preserved keyboard focus and existing text.
  The blocked and queued states were inspected at 1,280, 720 and 390 pixels using isolated data.
- [430](../tasks/430-account-block-and-restart-keep-the-work.md): the native Tauri and Electron restart proofs each
  produced one signed-in return and kept it after another reopen. The repeatable three-launch test now selects
  and names the shell at runtime. The hidden Electron fixture stayed below the disconnected 400 MB process-tree
  ceiling. Its account check and reply used fixtures.
- [389](../tasks/389-recover-shared-service.md): two active replies sharing a silent service now trigger a bounded
  local probe. With one active reply, a Stop for a known turn also qualifies when its completion notice is missing after
  five seconds, even if the service acknowledged the interrupt or sent a status update. A native MCP tool still open after
  15 minutes of silence or a native command still open after 30 minutes of silence qualifies as well. The one-reply checks
  preserve pending approvals and questions in other conversations.
  An unresponsive service is stopped and writer
  exit is confirmed before the ordinary Keep going retry.
  An unconfirmed queued delivery stays available for a deliberate send. Recorded-wire tests cover a healthy peer and
  no replay, and an isolated Windows test confirmed the real process tree stopped before affected replies were released.
  [The controlled recovery and resource measurements](shared-codex-service.md) distinguish the admission budget from production limits.

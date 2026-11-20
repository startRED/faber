# Autonomy checks and remaining work

The finite-wait repair is on main as `a7e3ee8bd5d62cf8b9f4ec84869b3b6c230009f6`.
Its [full remote verification](https://github.com/startRED/faber/actions/runs/43828218007) passed.
This page distinguishes the delivered behavior from the older installed app and remaining work.

## Confirmed defects and repair

Declared waits previously depended on an event without a finite fallback. They now persist a deadline;
startup restores it without resetting the clock. At the deadline the engine checks actual followed workers,
requests one continuation or saves a visible admission/delivery blocker. Uncertain delivery is not retried.
PR waits retain their head/state/push baseline, so passing checks on a new commit of the same PR and an
already merged PR return the waiter; stale readings do not. Healthy caller completion no longer cancels
actual helpers. Terminal helper reports preserve a bounded reason and partial output before delivery.
Bounded acknowledgements and checked writes prevent late control writes into a torn stream.

The actor behind the historical 137.743-second helper interruption was not recorded. A normal 900-second
silence limit does not explain it. The reproduced caller-completion bug is fixed; it does not establish
who caused that historical interruption.

## Evidence and limits

- Regression checks failed before the corresponding fixes. Final local proofs: 483 engine library cases,
  41 helper cases, 74 and 82 driver recordings, 13 focused UI cases and all seven project checks passed.
  The final 17 wait cases include no-event deadlines, restart receipts, account refusal, new heads and merges.
- The isolated live Haiku/low restart/account probe reported USD 0.006722 for its last successful run.
  A real read of a historical merged GitHub PR requested a return with zero model processes permitted.
- An isolated native Electron process restart preserved the exact saved deadline. Its restored timer
  reached visible account refusal without an external event or model turn; no-CLI memory was 128.12 MB.
  This does not prove a deadline expiring while closed, every shell, or a live-model native restart.
- The native app was exercised with synthetic data and inspected at 1280, 720 and 390 pixels. The final
  no-CLI fixture measured 166.55 MB, below the 400 MB test ceiling. No resident production process was added.
  CLI process-tree peak and incremental processor cost were not measured.
- An actual cheap-model helper stopped mid-task with automatic caller recovery and folder-state reporting
  has not been proved. Controlled helper and wire fixtures passed; they do not replace that live proof.

## Installed app and outstanding work

The monitor reported a missed wake for nine minutes after the revert's checks passed. That earlier saved
wait had already cleared when the person wrote, so its pre-message state could not be recovered. A subsequent
read-only inspection, immediately after this lane declared another wait at 08:09:49 UTC on November 16,
did observe only `note` and `since`, without `until`, in the installed app's own saved wait. These are
distinct observations. The isolated delivered code persists a finite deadline; the installed older app
was not updated or disturbed while real conversations were active.

- [400](../tasks/400-every-wait-has-a-wake.md): the person installs the update; then verify saved deadlines,
  autonomous return and truthful wait/blocked/completed UI with isolated data. Installation is not a source fix.
- [410](../tasks/410-a-stopped-helper-wakes-its-lead.md): complete the live interrupted-helper proof and
  missing folder-state reporting for an interrupted rebase or merge; verify/enforce the lead's live-work
  claim and retain observed stop causes without inventing unknown actors.
- [420](../tasks/420-the-screen-tells-waiting-from-stuck.md): finish distinct stuck styling and recovery
  actions beyond the delivered deadline/blocker words.
- [430](../tasks/430-account-block-and-restart-keep-the-work.md): complete native-shell restart/account
  recovery proofs; engine recreation alone does not establish all shell lifecycle paths.
- [389](../tasks/389-recover-shared-service.md): detect correlated shared-service stalls, bound its health
  check/restart and resume affected conversations without duplicate sends. This mechanism is not implemented.

The monitor observed 2.7 GB/537 threads with nearly unchanged CPU and silent conversations, followed by
2.53 GB/467 threads and estimated growth of 70-280 MB per 30 minutes; manual restarts restored progress.
These are monitor observations, not measurements of an internal leak by this lane. They do not justify
blindly killing a service based on memory alone. Its budget is separate from the no-CLI R125 ceiling.

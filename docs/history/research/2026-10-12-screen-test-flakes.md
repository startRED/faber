# Screen-test navigation races (2026-10-12)

## Evidence and cause

Run [38235233684](https://github.com/startRED/faber/actions/runs/38235233684), attempt 1,
failed the reduced-motion and narrow-window dismissal checks in 35.6 s and 34.3 s.
Both errors ended in `Runtime.evaluate. stderr:`, which comes from `Ctx::launch`,
before either scenario manipulates the column. Attempt 2 failed the permission-card
check instead. Run [38235227508](https://github.com/startRED/faber/actions/runs/38235227508)
passed the same commit, `3e1cba4`.

The local protocol trace shows the original client selecting `about:blank` and
issuing `Runtime.evaluate` before the app navigation finishes. Target discovery
did not distinguish the provisional document from the app. The client discarded
all navigation/load events and ignored errors from focus emulation. A connected
socket therefore did not establish that the page was ready for evaluation.

Reload checks had a second race: `location.reload()` returns before the old document
is replaced. A quota or permission already present could satisfy the next check in
that old document. Two waits discarded protocol errors; their outer 20/25 s limits
also could not interrupt a single 30 s evaluation. The shared send helper retried
Enter three times and similarly discarded evaluation failures.

The premature attachment is observed locally. The historical runner has no protocol
trace, so its exact renderer state at the unanswered evaluation cannot be recovered.

## Change

- Select the app URL, excluding the provisional blank page.
- Bound focus setup by the connection deadline and propagate its errors.
- Subscribe to page lifecycle events and await the main document's `load` event.
  Retain events received before a command reply. Match both frame and loader IDs.
- Reload through `Page.reload` and require a different main-document loader before
  evaluating the new screen. An old load or iframe load cannot release the wait.
- Use one startup deadline for discovery, loading and rendering. Keep the existing
  30 s local / 90 s CI startup budget, 30 s command limit and 15 s condition limit.
- Send Enter once and await the composer clearing. Quota waits now propagate errors
  and share the condition deadline with each evaluation.

No timeout was increased and no retry was added. Discovery still polls for a port
and the app target; arbitrary DOM conditions still use the existing bounded poll.
Navigation itself is awaited through protocol events. Chromium replays lifecycle
events when enabled, including documents loaded before attachment; see
[InspectorPageAgent::setLifecycleEventsEnabled](https://chromium.googlesource.com/chromium/src/+/main/third_party/blink/renderer/core/inspector/inspector_page_agent.cc).

## Local proof

Baseline: `f0bd964` on `phase-1-base`. All launches use the testkit's hidden desktop,
fresh sandboxes, fake CLIs and the normal containment/environment/write audits.
The nine scenarios are permission cards, a pending question at reload, both quota
reloads, all three dismissal variants, loopback installation, and the restored-chat
tutorial reload. They run sequentially, ten times before the fix and twenty after.

Before: **90 passed, 0 failed**. The intermittent CI timeout did not reproduce in
this local baseline. A separate diagnostic series also passed 27/27 while recording
the premature blank-page attachment.

After: **180 passed, 0 failed**, twenty passes for each of the nine scenarios.

All required gates passed: `npm run lint`, `npm run typecheck`, `npm run build`,
`cargo fmt --all --check`, both Clippy commands, and `cargo t --all`.
The full runner reported **607 passed, 0 failed, 0 not run, 0 skipped**. Its hidden
desktop probe found zero windows on the desktop in use. The full log is
`target/proofs/screen-full-proof.log`.

The target-selection regression fails when the original selector is restored:
`/devtools/page/BLANK` is selected instead of `/devtools/page/P1`. With the fix, all
nine focused CDP tests pass. The load regression delivers the initial load before
the command reply, then an old document load and an iframe load during reload. The
mock rejects an evaluation issued before the new main document loads.

Raw logs and the temporary repeat harness are under `target/proofs/`:
`screen-before.log`, `screen-trace.log`, `screen-after.log`,
`screen-regression-before.log`, `screen-regression-after.log`, `screen-repeat.rs`.
The harness is not shipped. Equivalent runs with the normal runner:

```bash
export RUSTUP_HOME='E:\Apps\rust\rustup' CARGO_HOME='E:\Apps\rust\cargo'
export PATH="/e/Apps/rust/cargo/bin:$PATH" CARGO_TARGET_DIR=target/proofs CARGO_INCREMENTAL=0
for round in {1..20}; do
  for filter in 'permission card' 'screen loads shows' 'after the screen reloads' \
    'column opened and closed' 'column closed with Esc' 'a reload shows' \
    'installs from loopback' 'still opening never gets'; do
    cargo t --suite screen --filter "$filter" || exit 1
  done
done
```

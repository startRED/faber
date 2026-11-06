# Overnight bug log, 2026-10-18

Problems found while watching Faber unattended. Each entry: what happened, how to reproduce, evidence, suspected cause.
Fixed items say how. Nothing here is a decision; it is a list to fix together.

## Method

- Hidden test build driven by `testkit/examples/ux_audit.rs` (fake CLI, no real subscription spent).
- Memory and CPU read with the example's `memory` and `cpu` ops.
- The owner's own running chats, read from the data folder (`conversations/<id>.json`).

## Baseline run (hidden test build, pt-BR, fake CLI)

- Starts clean: 90 MB memory, about 2 s CPU at idle on the home screen, no console errors seen.
- Screenshot looks right at 1280x800. One thing to look at: the empty "Projetos" list in the sidebar shows the mascot
  cropped by a line under it (low priority, taste only).

## Findings

### 1. Keep going stopped by itself (fixed on branch fix/keep-going-billing)

- Owner (2026-10-18): when a person turns on Keep going (the "Continuar sozinho" chip), they are trusting Faber to carry
  on while they are away. Quota pacing and other small things must not switch it off.
- What happened: two project chats stopped with `error.billingUnknown` and `reason.keepGoing.billed`. Claude Code sends
  its usage notice only when something changed. The engine took a turn without one as "billing unknown" and cleared the
  confirmed subscription, which also stopped the turn that was running. Separately, Auto's pace (spending fast or low)
  and an empty quota ended Keep going.
- Fix: a turn without a notice clears nothing (the 15-minute `claude auth status` check stays the truth); pace never
  stops it; out of quota it waits for the reset and stops only when none is known; a login billed by use still stops it.
  Decision R212 in `docs/next/fix-keep-going-billing.json`.
- Not tested: the wait for a known reset has no test yet (needs a clock the test controls).

### 2. A landed task's file claim blocked the fix (open)

- A third chat (Codex) tried to fix finding 1 and was stopped by another task's lane claim on `engine/src/claude.rs`
  (`steer-claude`, whose work had already landed). The owner's request waited for a claim nobody was using.
- Open: claims of landed work should clear by themselves, or the claim check should say who holds it and since when.

### 3. The chats stay stopped until a build with the fix is installed (open)

- The two chats the owner left on Keep going are halted now. They do not resume by themselves; each needs Keep going
  turned on again after the fixed version runs.

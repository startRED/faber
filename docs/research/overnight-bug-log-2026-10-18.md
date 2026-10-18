# Overnight bug log, 2026-10-18

Problems found while watching Faber unattended. Each entry: what happened, how to reproduce, evidence, suspected cause.
Fixed items say how. Nothing here is a decision; it is a list to fix together.

## Method

- Hidden test build driven by `testkit/examples/ux_audit.rs` (fake CLI, no real subscription spent).
- Memory and CPU read with the example's `memory` and `cpu` ops.

## Baseline run (hidden test build, pt-BR, fake CLI)

- Starts clean: 90 MB memory, about 2 s CPU at idle on the home screen, no console errors seen.
- Screenshot looks right at 1280x800. One thing to look at: the empty "Projetos" list in the sidebar shows the mascot
  cropped by a line under it (low priority, taste only).

## Findings

### 1. "Continue on its own" must not be switched off by quota pacing (owner requirement)

- Owner (2026-10-18): when a person clicks the control that lets Faber keep working autonomously, they are trusting it to
  keep running. Quota pacing for Codex or Claude, and similar minor reasons, must not turn it off or stop the run.
  Pacing may change which vendor or model runs the next step, or wait for a reset, but the autonomous run stays on.
- Where to look: the autonomous mission mode (`engine/src/project.rs`, `Autonomous`), Auto's pace reasons
  (`reason.pace.*`, `reason.waiting`, `run.waiting` in `ui/locales/en.ts`). Not yet confirmed which code path ends a run;
  no UI label with the exact words "Continue on its own" was found by search, so the control's name still needs to be
  identified.
- Status: open, needs investigation.

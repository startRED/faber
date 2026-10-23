# Frontend excellence implementation record

Status: in progress. Scope: the person's request to apply useful parts of the 243-technique blueprint and improve
the frontend expertise Faber uses for its own screen and generated interfaces. This is not approval for a new visual
identity, voice/remote infrastructure, copying third-party skills or speculative performance dependencies.

## Recorded input and audit

The complete ID inventory and static evidence are in the
[audit](../../research/2026-10-22-frontend-blueprint-audit.md) and its linked second part. This retains all 243 IDs,
including unknowns. The priorities P0/P1/P2 are the source study's proposals, not proof a feature is absent.
The [source study](../../research/2026-10-22-frontend-sources.md) records checked sources and unavailable pages.

## Current slice

- [x] Work from the current development snapshot in a separate checkout; preserve the older conversation checkout.
- [x] Record one status/evidence/recommendation for each of the 243 IDs, explicitly as static inspection.
- [x] Write original frontend-craft and frontend-proof skills with selective loading and project-aware boundaries.
- [x] Verify their delivery through the existing guide path for both vendors, fallback and permission modes.
- [x] Extend the existing Faber UI skill beyond motion with the same interaction and proof workflow.
- [x] Prove and repair same-file attachment reselection without changing attachment persistence.
- [x] Complete local proofs, independent code/security review and skill scenario review; record remaining limits.
- [ ] Queue the proven change through the normal workflow. Local checks do not establish a release.

## Remaining reliability work, in dependency order

These are unfinished implementation items, not automatically solved by instruction skills. Each needs its own narrow
contract and failing-before/passing-after proof, extending the existing send/draft/navigation path.

1. **Durable draft attachments (C03/S06/F05).** Persist references/bytes with bounded storage and cleanup in the
   existing draft store; cover reload, restart, removed files, failed storage, branch/conversation changes and privacy.
   Do not write large base64 blobs into localStorage or pretend an OS path alone preserves pasted files.
2. **Durable outgoing intent and reconciliation (R01/R03/R04/R05/T01/T07).** Establish stable message identity and
   durable acknowledgement at the engine boundary before an optimistic UI. Cover repeated submit, cancel/late ACK,
   crash at each persistence boundary and restart; never replay ambiguous external effects blindly.
3. **Exact reading restoration (S02/C15/C16/F05).** Extend existing scroll/navigation memory with a message anchor
   and relative offset. Cover late layout/image changes, streaming above the viewport, removed messages, branch
   switches and bounded storage. A durable raw scrollTop is not enough.
4. **Measure before optimization (P01/P06/P08/P14/M03/M10).** Record representative input/scroll/resize frame and
   memory/CPU baselines, including hidden state and long tool logs. Add only the optimization the trace justifies.
5. **Generated frontend evaluation (D13/M01/M14).** Run the source study's paired fixtures and independent visual
   review. Skills delivered successfully do not prove the generated interfaces improved.

Other partial/unknown audit rows stay open until their own evidence exists. Existing pane/tab/side-chat work is
extended rather than rebuilt. Voice, remote machines/devices, scheduling and comparator products are separate
capability work, not cosmetic improvements or claims of parity made by this slice.

## Local evidence for this slice

- Frontend/guide integration: 14 passed, including both vendors, both areas, native/index/fallback, guarded modes,
  resumed/compacted sessions and Windows junction rejection. The first expanded run found two index assumptions
  (old 400-byte bound and language suffix); both were corrected and the 14 passed again.
- File reselection: a new UI check failed before the change on the retained native fakepath; after the change the
  full UI suite passed, 687 checks, no failures or skips. File bytes and a single sent attachment are checked.
- Script checks: 101 passed, one skipped, zero failures. Types, source analysis, documentation, production bundle,
  formatting and lane claims passed. Source analysis has five pre-existing warnings outside the touched files.
- Two independent read-only reviews found no security blocker. A separate scenario review exercised six proposed
  skill plans by reading, without building sample interfaces; it found no scope or routing blocker.
- Before photos: eight real screenshots; the existing composition scenario passed without findings. A fresh
  reviewer scored all four screens at both sizes 40/40. The six affected before/after pairs were then compared by the same independent reviewer, 40/40 to 40/40, no findings.
- An after-photo attempt was invalidated by running the production bundle command concurrently: the harness
  detected the changed dist directory and refused the run. The serialized retry passed and produced the six affected after photos.
- The new skill bodies total 15,655 UTF-8 bytes; each discovery description is <=160 bytes. This is text size,
  not a whole-app memory/CPU measurement. No resident process, library dependency, timer or visible window was added.

Receipts remain in the working copy's target/frontend-guide-cargo.log, target/faber-test/run.log and
target/faber-test/critique/. The UI suite's 687-result receipt was produced before screenshot runs reused run.log;
the tool log retains its full result. Queue-wide and generated-design quality measurements remain unverified.

# Visual roadmap and implementation order

Status: Proposed experience. All examples below are illustrative, not live progress or implemented screens.

## The one question

What can I use now, what is Faber doing, and what is keeping the rest from being ready?

Answer inside current Projects and its native saved plan. Start with a sentence and a compact vertical outcome
route. Open detail in the current viewing/running surface when requested. Do not add a fixed dashboard, workflow
board, agent network, new application area or competing running list.

## A concrete sketch

The following is example content, not the status of Faber or this proposal:

```text
Faber / Your project
The export works. Faber is checking a large file.

2 of 5 required outcomes verified · scope revision 3

✓ Understand the input
✓ Make the export work             Open result
● Handle a large file               Checking
○ Add the preview                   Next, after the file check
○ Prepare final delivery            Later

Needs your decision                shown only when a real decision exists
Which preview style should stay?   Open comparison

Details: request · files · evidence · choices · cost · history
```

Show outcomes people recognize, not a live org chart of agents. Expand one milestone into its existing subplans,
keeping the current 12-step plan limit. A long project does not require a huge list mounted in the conversation.
Live conversations and helpers remain in the running column. Model/effort remains in the composer; persistent
settings remain in Settings. Priority and scope changes are work actions that record a new plan revision.

The native plan already counts reported steps. The proposed verified-outcome count must be labeled separately
until those steps have trustworthy evidence links. Never relabel an agent-written checkbox as engine verification.
Use the current status/closing-box and artifact grammar; do not create a second completion receipt.

## Honest progress

- Count verified required outcomes for the selected accepted scope; show numerator and denominator together.
- Do not treat uneven task counts as percent of time/effort remaining.
- Checking/review is not done. Published availability is a separate delivery fact.
- Ideas, optional, deferred and canceled work remain explainable; never mark them complete to improve a metric.
- A scope revision says what changed. Splitting tasks cannot manufacture progress.
- Relevant input changes invalidate current acceptance while retaining historical evidence.
- Missing execution data says unknown. A worker's claim is reported, not verified.
- Give time ranges only after comparable recorded runs calibrate them; otherwise show dependencies without an ETA.
- Show incomplete cost honestly, without equating API-priced token use with a subscription bill.

Do not show a global completion percentage for the evolving complete Faber vision. A milestone's optional bar
represents accepted outcomes in its selected scope, with that meaning written next to it.

## Accessible and light

Use existing Faber theme tokens, controls, keyboard focus and disclosure behavior. Amber means needs the person;
vendor colors retain their current meanings. Every state has text and a shape as well as color.
At 720 px, keep the route vertical and the conversation reachable. Long translations wrap without losing meaning.
Exported summaries read on a phone; that does not introduce a phone executor.

Render only opened detail and page long histories. No animation is required. Reduced motion, hidden window and
a game in front preserve existing zero-repaint behavior. Reuse current attention rules instead of toasting every
completed item. All eventual product text belongs in the five locales, authored by hand.

Cover no plan, no AI, no active work, all done, missing evidence, stale output, unavailable quota, permission needed,
canceled work, corrupt records and missing artifacts. Show what is known and the condition that changes the state.
An optional export is a dated snapshot through the existing file flow, never another source of plan status.

## Six implementation slices

All are proposed. Preserve the order of the four approved R435 slices and do not fork or replace their work.
Read-only discovery can proceed now; contracts, trust, adapters and review-gate extensions depend on their relevant
approved slices being completed. This roadmap does not reorder Faber's main product phases.

```text
F0 Current truth
  -> F1 One readable view
    -> F2 Durable outcome links
      -> F3 Reliable coordination
        -> F4 Full-product acceptance
          -> F5 Comparative qualification
```

### F0 — Establish the current truth

Pin current engine/model/store schemas, product decisions and the approved work-method revision. Resolve only the
documentation discrepancies this feature relies on. Map the native plan, goal, check, memory and artifact records.
Build fixtures for current conversations, legacy plans and incomplete evidence.
Exit: a reviewer can identify one owner per fact; no mission revival or competing queue.

### F1 — Make today's work understandable

Improve the current native plan/detail using facts already available. Distinguish reported steps from verified
results; show current artifacts and missing links honestly. Reuse the existing running and closing surfaces.
Exit: a non-programmer identifies now/next/blocker; no fabricated historical verification; keyboard, width,
long-language, reduced-motion and idle-resource checks pass. No new scheduler is required for this slice.

### F2 — Add stable outcomes and a persistent route

Extend saved plans with stable step/outcome IDs, acceptance source refs and revision history. Add versioned milestone
membership under the existing project record, referencing those plans. Bind applicable verifier results/review
receipts to outcomes. Keep original requirements intact and use approved adopted task sources.
Exit: fresh sessions resume with complete required context; concurrent edits cannot silently overwrite scope;
changed inputs invalidate affected acceptance. No `.faber/plan`, roadmap task database or copied backlog.

### F3 — Coordinate safely across conversations

Use existing helpers, started conversations, separate copies, budgets and repair behavior. Add validated dependency
readiness, integration checks and candidate invalidation across plans. Strengthen write-scope ownership where needed
in the current architecture. Measure admission headroom and reserve independent-review capacity.
Exit: cancellation, crashes, quota loss, overlapping writers and changed dependencies cannot produce false accepted
outcomes or blind replay of ambiguous external effects. Do not claim process guarantees borrowed from removed code.

### F4 — Establish complete-product acceptance

Apply the same outcome links to representative research, visual assets, rendered documents, media and automation.
Add only checks supported by actual installed tools. Exercise a long-running project across session compaction and
vendor switching. Optional user-requested reproducibility files are deliverables, not new memory-library writers.
Exit: supported work has concrete evidence and an explicit human-judgment boundary; unsupported checks stay not run.

### F5 — Compare, simplify and qualify

Compare with current Faber, plain CLIs and pinned BMAD/GSD versions using [evaluation.md](evaluation.md).
Remove stages that spend more without improving accepted results. Keep legacy reading and a read-only fallback.
Exit: independent scorecard reports quality, total tokens, corrections, recovery and usability, with losses included.
The owner chooses adoption; no performance claim is made merely because a design looks comprehensive.

## Ranked priorities after preparation

| Rank | Capability | Impact / effort / uncertainty | Trade-off |
| :--- | :--- | :--- | :--- |
| 1 | Clear current native progress | High clarity; moderate effort; bounded integration risk | Does not automate more work yet |
| 2 | Outcome IDs, source refs and evidence links | High continuity/trust; moderate effort; schema risk | Needs compatibility and R435 integration |
| 3 | Cross-plan dependencies and integration checks | High coordination value; high effort and concurrency risk | Wait until records are reliable |
| 4 | Domain-specific acceptance | Full-product fit; variable effort/tool availability | Admit only what can be verified |
| 5 | Comparative evaluation and simplification | Establishes value; recurring benchmark cost | Avoids unsupported superiority claims |

Defer a workflow editor, role marketplace, cloud sync, separate dashboard service, vector memory and elaborate
estimation engine. None is necessary to show honest progress or resume useful work.

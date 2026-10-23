# Visual progress and implementation roadmap

Status: Proposed experience and implementation slices. Illustrations are not live project status.

## The screen's question

What can I use now, what is Faber doing, and what is keeping the rest from being ready?

Make that answer readable in one glance. Start with a sentence, an outcome route, current work and the next ready
outcomes. Open details only when requested. Extend Projects and its current project context; do not replace the
conversation, add another application area or resurrect the removed mission board.

## Proposed view

This sketch uses illustrative states to explain behavior, not measured completion of Faber or this proposal:

```text
Faber / Your project / Progress

The export works. Faber is checking how it handles a large file.
2 of 5 required outcomes accepted · scope revision 3

✓ Understand the input
✓ Make the export work             Open the result
● Handle a large file               Checking · evidence in details
○ Add the preview                   Next, after the file check
○ Prepare the final delivery        Later

Now
Checking a large file. The preview can start after this passes.

Needs your decision                 only present if a real decision exists
Which preview style should stay?    Open the comparison

Details                             original request, files, evidence, cost, history
```

Use a vertical sequence of outcomes, not a board with workflow columns or an agent network. Order can show
dependencies without suggesting all work is serial. A person can expand a milestone into its outcomes and open
one outcome's current explanation, deliverables, proof, decisions and history.

The existing running column owns live conversations, helpers and attention. Progress references those entries,
not a second list of agents. Model selection stays in the composer; persistent controls stay in Settings.
Priority or scope changes are work actions, recorded as plan revisions, subject to existing permissions.

## Honest progress

- Count accepted required outcomes for the selected scope revision. Show the count and denominator together.
- Do not treat a fraction of differently sized tasks as percentage of effort or time remaining.
- An outcome awaiting required review is checking, not done. Published availability is a separate delivery fact.
- Deferred, canceled and optional work stay visible in details and are excluded with a reason, never hidden as done.
- If scope changes, show what changed and its new revision. Splitting tasks cannot create apparent achievement.
- A dependency or criteria change marks affected acceptance stale; retain the old acceptance in history.
- If execution facts are missing, say unknown. A worker's self-reported progress is not verified progress.
- Show a range for time only once calibrated with comparable runs. Otherwise use the next dependency and no ETA.
- Mark cost completeness and capture time; never equate API-priced token estimates with a subscription charge.

For a large milestone, an optional bar may show accepted outcomes within the current frozen scope, labeled as such.
Do not show a global percentage for the entire evolving Faber vision. Detailed task counts belong behind disclosure.

## Interaction and accessibility

Use Faber's existing theme tokens, typography, disclosure patterns and focus behavior. Preserve the reserved colors:
amber means needs the person; vendor colors identify the vendor. Every state also has text and a shape.
Use real buttons and links with keyboard focus, descriptive names and a stable reading order.

At 720 px, keep the route vertical and the conversation reachable. A standalone exported summary must also read
well on a phone; that does not promise a phone execution app. Long translations wrap rather than truncate meaning.
No animation is required for progress. With reduced motion, hidden window or a game in front, stop visual activity
and avoid background layout work. Render only opened sections and page long lists.

Required empty and failure states: no plan, no AI connected, planning not accepted, no work running, all done,
permission needed, missing dependency, quota unavailable, corrupted record, stale summary and unavailable artifact.
Each says what is known and what changes the condition. Do not show an empty gray dashboard or fake activity.

No completion toast for every task. Reuse existing attention rules. A stopped autonomous run retains its existing
notification behavior. All product text is authored in the five locale files as part of implementation.

## Portable human view

Generate `.faber/README.md` from the same view used on screen. It includes the objective, accepted scope revision,
capture time and freshness, accepted outcomes, now/next, decisions, current delivery links and evidence references.
It is useful outside Faber but never a source of execution authority. It must state when it is merely a snapshot.
Keep the README small and link to details. A manually edited generated summary is not imported as a status update.

## Roadmap for building Faber Flow

All six slices below are proposed and unstarted by this task. Their order does not replace the main product roadmap.
Schedule them within the existing self-development, Full Chat and Projects phases at the relevant boundaries.

```text
F0 Establish truth
  -> F1 Show current evidence
    -> F2 Make plans portable and briefs small
      -> F3 Coordinate several bounded tasks
        -> F4 Prove full-product use
          -> F5 Compare and qualify adoption
```

### F0 — Establish truth and compatibility

Inventory the current project, conversation, mission and development-queue interfaces. Resolve the specific living
document contradictions affecting this feature. Pin current schemas, owner decisions and recovery contracts.
Create representative recorded fixtures, including incomplete conversation-only work and legacy one-task missions.

Exit: a reviewer can identify the owner of each fact and the relevant existing check; no undocumented migration.
No new UI or autonomous behavior. Dependency: none; scoped to this feature's foundations.

### F1 — Show current work from existing evidence

Build a read-only projection over existing records and expose a minimal project progress detail through Projects.
Show what can actually be known for conversation-only work. Generate the portable summary from that same projection.
Use a proposed definition only where the old data cannot represent outcomes; never fabricate historical completion.

Exit: UI and summary agree on a pinned generation; a planted false worker success cannot mark an outcome done;
stale, absent and corrupt data remain distinct. Pass relevant layout, memory and zero-idle-work checks.
This is the first useful slice; no multiple-writer scheduler is needed to understand today's work.

### F2 — Make plans durable and worker briefs small

Add roadmap/item definitions, original-request references, stable acceptance IDs and controlled planning edits.
Compile focused briefs through the existing kit system. Add the adapter for Faber's current task queue.

Exit: a fresh session resumes without reading the full history; required words survive byte for byte; conflicting
edits are refused; legacy source ownership is unambiguous. Measure brief size and resumed-task correctness.

### F3 — Coordinate multiple tasks with bounded repair

Extend the current one-task coordinator explicitly. Add ready-task selection, resource admission, dependency
invalidation, bounded repair, independent review capacity and final integration proofs. Keep existing grants,
process leases and effect reconciliation. Integrate the approved verifier as its own slices become available;
do not assume its unfinished stages are a prerequisite for every read-only improvement.

Exit: crash/restart, cancellation, changing inputs, quota exhaustion and overlapping paths cannot accept stale
output, duplicate a reserved effect or run an unauthorized writer. Show measured peak resources including workers.

### F4 — Prove the complete-product contract

Use the same records for representative research, visual assets, rendered documents, video and automation tasks.
Add domain-specific acceptance only where installed tools can actually produce and verify the outputs.
Exercise a long-running project across compacted contexts and vendor changes.

Exit: every supported kind has a concrete check recipe, clear human-judgment boundary and linked artifact;
unsupported checks remain explicitly not run. No new runtime, driver or bundled catalog content.

### F5 — Compare, simplify and qualify adoption

Run the controlled comparison in [evaluation.md](evaluation.md), including contemporary BMAD and GSD versions.
Remove stages that add cost without improving accepted outcomes. Keep a read-only/legacy fallback while adoption
is evaluated. Let the owner decide product direction and any default workflow change from measured results.

Exit: publish a truthful internal scorecard with quality, total token cost, corrections, recovery and usability.
If targets fail, show the regression and keep the proven previous path. Do not rename a failed comparison a win.

## Why these five priorities come first

F0 is mandatory preparation. Rank the next five capabilities without invented numerical scores:

| Rank | Capability | Impact / effort / uncertainty | Trade-off |
| :--- | :--- | :--- | :--- |
| 1 | Evidence-backed read-only progress | High trust; moderate effort; bounded integration risk | Less automation initially |
| 2 | Durable intent and focused briefs | High continuity/economy; moderate effort; schema risk | Requires strict ownership and migration |
| 3 | Multiple tasks and bounded repair | High throughput; high effort and concurrency risk | Delayed until truth and recovery hold |
| 4 | Cross-domain deliverable checks | Broad product fit; variable effort and verifier availability | Only admit supported kinds |
| 5 | Comparative qualification and simplification | Establishes whether adoption is justified; recurring evaluation cost | Avoids unsupported superiority claims |

Defer a visual workflow editor, custom role marketplace, separate dashboard application, cloud synchronization,
vector memory store and elaborate estimation system. None is required to make progress truthful or work resumable.

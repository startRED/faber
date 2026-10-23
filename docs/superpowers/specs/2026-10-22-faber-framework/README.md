# Faber Flow: a framework designed for Faber

Status: Proposal. Nothing in this folder implements or changes the product.
Date: 2026-10-22. Working name: Faber Flow; naming remains a product choice.

## Read this first

Faber Flow connects intent, planning, execution, evidence and delivery through Faber's existing engine.
People see a readable route to their result. Agents receive the smallest complete brief for their current task.
The engine owns execution facts; the writer cannot turn a claim into a successful check.

This proposal covers both building Faber itself and the complete intended Faber product, including non-code work.
It does not claim that future capabilities already exist. The repository snapshot, rather than an installed build,
was inspected. The user's request is preserved in [request.md](request.md).

| Read | To answer |
| :--- | :--- |
| [Baseline](baseline.md) | What exists, what is planned, and what BMAD and GSD contribute |
| [Records](records.md) | Where files live, what owns each fact, and how state survives interruption |
| [Execution](execution.md) | How work is selected, delegated, checked, resumed and delivered |
| [Roadmap and experience](roadmap.md) | What the person sees and the order in which to build it |
| [Evaluation](evaluation.md) | How to prove quality, economy, recovery and usability |

## 1. Summary

Use one small framework inside Faber, not another orchestration application or a bundled methodology.
Extend the mission journal, worker envelopes, independent review, Auto routing and existing progress surfaces.
Make current work understandable without reading a conversation or opening technical files.

The central relationship is:

```text
Original request -> accepted outcomes -> planned work -> attempts -> evidence -> accepted delivery
                          |                 |              |                        |
                      decisions        dependencies     live activity         files and recipe
```

The roadmap is a projection of these relationships, never an agent-written second status ledger.
An accepted delivery and a published version are separate facts.

## 2. Background

Faber already has durable mission records, contracts, scoped workers, receipt-backed acceptance and process leases.
Its current coordinator deliberately accepts one task and one writing attempt. Projects currently opens conversations;
the old mission board is absent. The development task queue, conversation plans and mission records do not yet form
one durable, user-readable project plan. Some living documents contradict the current code; see the baseline.

The opportunity is to connect those pieces with a reliable planning model and an understandable view, rather than
to add more roles, prompts or dashboards. Planning itself is a valid deliverable and need not start implementation.

## 3. Objective

Quality comes first, with economy close behind and speed after both, following Faber's product principles.
Use the existing product targets as the desired destination, not measurements of this proposal:

- Win or tie nine out of ten Projects requests against the plain CLI, with independent assessment and executable proof.
- Reach at most 70% of the plain CLI's token cost per accepted task without reducing quality.
- Cut corrections requested after completion by half. Never report an unrun check as passed.

Framework-specific acceptance also requires honest progress, exact-version evidence, safe recovery and quick
orientation for someone who does not program. Thresholds and comparison design are in the evaluation document.

## 4. People and their jobs

| Person | Job | Constraint |
| :--- | :--- | :--- |
| Someone delegating a result | Understand what is ready, happening, next, or needs a decision | No commands or project-management vocabulary required |
| A frequent user with several projects | Change priorities, inspect outputs and resume work | Stable identities and no duplicate progress lists |
| Faber's maintainer | Build Faber through Faber and verify the exact result | Existing product rules, checks, review and landing authority remain in force |

Agents are consumers of the same project records, not another audience with a separate project truth.

## 5. Value propositions

| Segment | Why | Before | How | After | Alternatives |
| :--- | :--- | :--- | :--- | :--- | :--- |
| Delegator | Wants a finished result without supervising every step | Reconstructs progress from chat | Outcome-based route, current facts, linked outputs | Can understand and redirect the work | Direct AI conversation or a generic framework |
| Frequent user | Needs continuity across sessions and projects | Repeats context and tracks several lists | Durable records, bounded briefs, explicit dependencies | Resumes without reconstructing history | Manual notes, GSD or BMAD artifacts |
| Maintainer | Needs fast development without accepting unchecked changes | Connects queues, documents and proofs manually | Adapters into the same evidence model | Can see exactly what was checked and accepted | Existing scripts used separately |

The differentiator to test is continuity from the person's words to the delivered result inside Faber, combined
with native resource accounting and engine-enforced evidence. A methodology alone does not prove better results.

## 6. Proposed solution

### A. A common spine with task-specific checks

Keep request, outcome, task, attempt, decision, evidence and delivery as a small shared vocabulary.
Software, research, image, video, documents and automation differ in artifacts and acceptance checks, not in
project-management machinery. Short Chat answers do not create a project tree. Durable multi-step work does.

### B. Planning sized to risk

A small reversible change needs a short outcome and relevant checks, not a full specification ritual.
Multi-area work adds dependencies and contracts. Security, data migration, money and external effects add the
appropriate isolation, review and authorization. Risk can escalate the path; an agent cannot lower required checks.

### C. One place for each fact

Existing mission records remain the execution spine. Human-readable task definitions describe desired work;
accepted plans pin them to an attempt. The screen and a portable Markdown summary read the same projection.
Private sessions, grants and live process state remain in Faber's private data, outside the repository.

### D. A route people can understand

Extend Projects with an optional progress detail, accessed from the project context, showing outcomes in order.
Lead with now, next, finished and the one decision that matters. The existing running column continues to show
live conversations and helpers. Avoid a new agent map, a wall of cards or another settings surface.

### E. Independent completion

Workers submit candidates. Engine-verified checks and an independent review determine whether a candidate is
accepted. Creative taste and product direction belong to the person. Publishing and spending require authority;
finishing a task is not permission to publish it.

## 7. Alternatives considered

| Approach | Advantage | Why it is not the proposed default |
| :--- | :--- | :--- |
| Prompt files only | Cheap and portable | Cannot enforce runtime state, process ownership or truthful completion |
| Large role-based pipeline for every request | Thorough specialization | Coordination and context overhead for simple work |
| File-first planning connected to Faber's engine | Human-readable intent plus enforceable execution | Requires careful adapters and schema evolution; recommended |
| Separate workflow service and database | Independent scaling | Duplicates Faber's engine, adds idle cost and synchronization failures |

BMAD and GSD already address some of these concerns. The proposed advantage is specific integration with Faber;
comparative superiority remains unproven until the evaluation passes.

## 8. Scope and adoption

Build in the six outcome slices in [roadmap.md](roadmap.md). First prove a read-only projection of current work,
then controlled planning and context packets, then multiple tasks, then full-domain adaptation and comparison.
Do not reorder Faber's overall product phases or adopt the proposed files as production formats in this planning task.

No new always-running process, cloud account, vendor driver, framework dependency or permanent team hierarchy is
proposed. Use the current Rust engine, SolidJS surfaces and open files. Optional tools still come from the catalog.

Assumptions: the framework should serve Faber itself and user projects; current product constraints remain binding;
the requested visual roadmap is an outcome route, compatible with the existing rejection of agent maps and boards.
These are design assumptions, not owner-approved rules. This proposal does not adopt a new product decision.

The main unresolved taste choice is the final location and presentation of progress inside Projects. The proposal
gives a concrete option to review; no existing screen is changed. Implementation estimates need the first slice's
measured results; no calendar dates or unmeasured performance claims are promised.

# Execution and quality rules

Status: Proposed extensions to the current engine. No new runtime is implemented by this document.

## One control path

The lead clarifies intent and coordinates. Workers make bounded candidates. Existing engine services own process
admission, grants, leases, verified effects and acceptance. A reviewer works in a fresh read-only session and does
not receive the writer's persuasion as evidence. The person owns direction, taste, money and reserved actions.
These are responsibilities, not five always-running agents.

For Faber self-development, the existing development queue remains the landing authority. For other projects,
mission acceptance remains scoped to the mission's own branch or output. Neither implies publication.

## Choose the smallest safe path

| Path | Trigger | Required preparation | Completion |
| :--- | :--- | :--- | :--- |
| Answer | Question or lightweight conversation | Relevant context and sources | Answer with known limits; no project scaffold |
| Small task | One bounded reversible result | Short outcome, scope and applicable checks | Evidence plus review required by existing rules |
| Coordinated work | Multiple deliverables, areas or sessions | Dependency graph, interfaces and scoped briefs | Per-item evidence, integration checks and independent review |
| Sensitive work | Security, migrations, destructive effects, money or external publication | Relevant threat/data/recovery plan and authority checks | Stronger proof, required independent reviews and authorized delivery |

Use uncertainty, reversibility, data sensitivity and integration surface to classify work. Do not classify solely
by lines changed. Elevate the path when new risks appear. Do not lower review or safety requirements to save tokens.
Planning-only requests end with a reviewed plan; they do not authorize implementing that plan.

## Project planning

Translate the request into user-observable outcomes while keeping its original wording linked. Separate accepted
requirements from ideas. Each planned item needs an outcome, scope, criteria, source, dependencies and risk.
Technical uncertainty gets a bounded investigation with a decision criterion, not a fictional implementation date.

Before a visible Faber change, use the product map, rules, relevant design page and taste cards. Record the exact
entry extended. Contradicting an owner decision requires the person's decision, not an AI amendment.
The framework may propose follow-up work; it cannot silently expand the authorized goal.

Plan the immediate next slice in execution detail. Keep distant milestones at outcome level until their inputs
stabilize. Reject dependency cycles. A milestone is accepted only when all its required current outcomes are accepted;
optional ideas do not inflate its denominator. Scope changes create a new plan revision with an explanation.

## Scheduling and economy

Use dependency readiness first, explicit owner priority second, then the existing quality floor and resource limits.
Take the highest-priority ready item that fits authorized quota, measured RAM and CPU admission. Work on independent
items while checks or reviews run, respecting disjoint write scopes and reserved review capacity.

Default to one writer until there is independent ready work and measured capacity. Add workers only if the saved
work exceeds their briefing, duplicated context and integration cost. Independent research may run concurrently;
serial dependencies stay serial. Parallel work is not a quality metric.

Use Auto's actual capability and quota data; do not create fixed department-to-model assignments. Reading can use a
cheaper qualified model, complex changes a stronger one. Track attempt cost, review cost, retries and cache effects.
Escalate on specific failed evidence or uncertainty, not on every task. Stop new discretionary work at the budget
limit, settle active effects safely, and show incomplete cost as unknown. No unattended paid-credit fallback.

## Attempt lifecycle

Map these semantic stages onto the existing versioned engine records rather than adding an independent status file:

```text
proposed -> accepted plan -> ready -> working -> checking -> accepted candidate -> delivered
                                       ^          |
                                       +-- bounded repair
```

Pending decisions, failed dependencies, unavailable quota, stopped work and canceled scope are explicit conditions.
A resource wait is not a request for the person unless their decision is actually necessary.
The UI can group conditions into simple labels while technical detail preserves the exact cause.

The existing coordinator's one-task/one-attempt limit must be extended and tested before any multiple-task or repair
path is admitted. An engine that does not support the proposed path refuses it rather than simulating success in text.

## Engine-controlled completion

For each acceptance criterion, record `pass`, `fail` or `not-run`, its evidence and candidate digest. Keep a separate
reasoned `not-applicable` classification, agreed before acceptance; it is never converted into a passing check.
For a bug, use a relevant reproducer that fails before the fix when feasible. For creative work, define a concrete
bar rather than a meaningless test that an output file exists.

Acceptance requires the pinned scope and criteria, verified applicable checks, an independent review when required,
and artifact integrity for the exact candidate. Security changes retain the project's two-review requirement.
Prefer another vendor when available, following R146; a same-vendor fresh read-only review is the documented fallback
when appropriate. Reconcile the current coordinator's stricter other-vendor restriction before enabling that fallback.
Never silently waive review because quota is low; record the pending condition.

Unrelated pre-existing failures remain visible. They do not count as new passing evidence or silently disappear.
An accepted exception requires the authority the project specifies and is shown with the delivery.
Every finding needs a reproducible failure or concrete evidence to block. Optional improvements do not stall delivery.

Any material change to the candidate, requirements or relevant dependency invalidates affected evidence and review.
Run the necessary integration checks after combining outputs; two independently checked parts do not prove their
combination. Distinguish accepted locally, accepted into the destination and published/installed, including versions.

## Bounded repair and recovery

Use a proposed initial cap of two repair attempts per item, additionally bounded by the authorized spend and time.
Treat that number as a tunable hypothesis for evaluation, not an adopted owner policy or a reason to abandon easy work.
Classify the failure first: implementation defect, missing context, broken environment, unstable check or wrong plan.
Each retry needs a changed hypothesis or concrete correction; repeating the same failed command is not progress.

For implementation errors, repair the candidate. For a wrong requirement, amend the plan under the appropriate
authority. If the limit is reached, preserve the candidate and evidence, report unfinished work and its cause, and
continue only independent authorized items. The framework cannot manufacture approval through repeated reviewers.

On a crash, reconstruct from existing private run state, intent/receipt reconciliation and process-aware leases.
Never replay an ambiguous external effect or transfer a lease solely because its timestamp is old. On cancellation,
stop admission, terminate or settle children using existing lifecycle rules and preserve recoverable output.
Changing permissions or revoking a grant stops the next prohibited effect, including a queued worker's action.

## Across the complete Faber

| Kind | Examples of acceptance evidence | Human contribution |
| :--- | :--- | :--- |
| Software or mods | Reproducer, relevant checks, newcomer path, rendering and resource measures | Product intent; final direction |
| Research | Primary sources, date, claim-to-source mapping, explicit uncertainty | Decisions and trade-offs |
| Images and design | Dimensions, output inspection, brief coverage, provenance | Taste and identity judgment |
| Documents and spreadsheets | Rendered pages, formulas/recalculation, source consistency | Content and business choices |
| Video, audio and 3D | Render/export success, duration, timing, clipping and sampled visual inspection | Creative approval |
| PC automation | Preview/dry run where possible, effect receipt, recovery route | Authority for reserved external actions |

Use installed capabilities or catalog tools, never bundled third-party frameworks. Reuse the browser's existing
handoff for logins, captchas and payments. Domain-specific execution is admitted only once its checks exist;
the framework's common records do not make an unsupported tool or domain verifier available.

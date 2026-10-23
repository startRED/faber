# Acceptance and comparative evaluation

Status: Proposed measurement protocol. No comparative benchmark or runtime performance measurement was run here.

Revision 2 qualifies adaptive work across model capability levels and domains, not only roadmap usability.

## What would make this better?

Better means more accepted, correct outcomes for the same authority and resources, with less total token cost and
less supervision. File count, prompt length alone, number of roles and parallel workers do not establish quality.
The targets below are proposed evaluation criteria, not measured results or adopted product policy.

## Compare fairly

Compare Faber Flow, the current Faber workflow, plain supported CLIs, and pinned contemporary BMAD and GSD setups.
Use the same starting repository, task wording, acceptance bar, model versions, effort, tool permissions and budget.
Record unavoidable integration differences. Do not penalize a coding framework for lacking Faber's non-code UI;
compare BMAD/GSD on their overlapping software scope and report full-product coverage separately.

Pin BMAD, GSD Core and GSD Pi separately; compare releases on the same date, recording runtime/model/tool versions.
Historical original/GSD 2 failures are regression fixtures, not substitutes for testing their current successors.
If a competitor can be configured for a non-code task, evaluate that actual configuration and disclose its setup
cost. Do not score unsupported work as a fabricated competitor failure. Measure Faber's value there against the
same plain-model/tool workflow and the best applicable available configuration.

Run the product's rotating sample of roughly eight requests first; use the full roughly forty-task corpus for major
qualification. Include an existing-code bug, a visible change, cross-area work, interruption/resume and integration
failure. The full-product sample additionally covers research, images, documents and media where tools are available.
Counterbalance order and repeat variable cases. Keep baselines per model/tool version and record cache conditions.

Price actual reported input, output and cache tokens with a dated price table; include coordination, retries,
review and failed attempts. Unknown usage stays unknown and excludes a definitive cost-win claim for that pair.
Respect the approved evaluation budget and shared-account pacing. No unapproved API credit or unattended paid usage.

Use blinded outcome assessment, executable evidence where possible and the person's blind taste judgment for
creative results. Do not use the framework author as the sole evaluator. Keep raw receipts linked in the scorecard.

## Scorecard

| Measure | Proposed target | What a miss means |
| :--- | :--- | :--- |
| Projects quality | Product target: win/tie at least 9/10 against plain CLI | No quality-superiority claim |
| Total token cost per accepted task | Product target: at most 70% of the matched plain-model CLI, using a dated API-equivalent price table | No economy-superiority claim |
| Corrections after completion | Product target: half of plain CLI | Revisit acceptance and intent capture |
| Recovery integrity | Zero duplicated reserved effects or lost accepted results in fault cases | Blocks adoption |
| Truthful completion | Zero unsupported passes or stale-current acceptance in planted cases | Blocks adoption |
| Context continuity | Fresh worker satisfies every pinned required passage and constraint | Missing required context blocks adoption |
| Human orientation | At least 4/5 representative non-programmers identify now, next and blocker in 10 seconds | Redesign the view; small-sample formative target only |
| Comparison with BMAD/Core/Pi | Meet the [revision 2 qualification gates](qualification.md) against each eligible pinned competitor | No superiority claim from parity alone |

Nine out of ten is the product target, not a statistical conclusion from eight requests. Report actual sample
counts, ties, losses and uncertainty; run sufficient relevant cases before claiming the target is established.
Keep accepted scope constant within a comparison. Canceling hard work cannot improve the completion metric.

## Required failure scenarios

| Scenario | Expected result |
| :--- | :--- |
| Worker says all checks passed without receipts | Remains checking/unverified |
| Required check failed or was never run | No acceptance; reason visible |
| Candidate changes after review | Affected checks/review stale; history retained |
| Combined work breaks integration | Individual success cannot accept the combination |
| Dependency or acceptance definition changes | Affected descendants invalidated or replanned |
| Crash between intent and external effect receipt | Reconcile; do not replay an ambiguous effect |
| Crash between project and conversation updates | Reconcile revisions; no false current projection |
| Old worker remains alive after apparent timeout | No competing writer admitted based on time alone |
| Two writers claim overlapping case/symlink paths | Refuse unsafe concurrent admission |
| Two editors save the same planning revision | One succeeds; the other receives a conflict |
| Newer schema or corrupt journal | Refuse unsafe interpretation; retain original data |
| Quota ends or permissions are revoked | Stop prohibited admission/effects; preserve resumable work |

Also test cancellation during checks, an unavailable artifact, incomplete usage, path traversal, external-source
instruction injection, legacy adapters, export privacy, and an unsupported domain-specific check.
Add tests alongside behavior during implementation; this planning task does not add empty tests for proposed code.

## Resource proof

Keep the existing R125 limits: the normal app stays below 500 MB and its end-to-end check fails above 400 MB.
Measure app memory separately from CLIs and verifier workers, and also report their total peak so moving cost outside
one accounting boundary cannot hide a regression. Include private working set, CPU, process/handle count and VRAM
where relevant. Use the same machine and dataset before and after.

Test cold start, idle visible, hidden/tray, a large closed roadmap, a large expanded plan, a 1,000-message conversation
and representative concurrent workers. Start scale tests with 100, 1,000 and 10,000 outcomes using recorded data.
Measure projection time, visible update latency and retained memory after repeatedly opening/closing details.

Proposed screen budget: p95 input-to-visible-update below 100 ms for an already-open local view, on the recorded
reference PC. This is a hypothesis to validate, not a promise. Hidden views must do no animation or recurring repaint;
events may update bounded engine state without keeping a large DOM alive. Resource admission uses measured worker
peaks and an explicit headroom, not a fixed number of agents for every PC.

The plan itself adds no runtime code or processes. No new RAM/CPU performance measurement is claimed for it.

## Usability and coherence

Exercise first use, no AI, no plan, incomplete evidence, long translations, keyboard-only navigation, 720 px width,
reduced motion, a hidden window and a game in front. Include a frequent user reprioritizing a large project.
Check C001/C002/C003/C005/C006/C008/C010/C011 and relevant shared-control rules. Name exact evidence in the review.
The proposed route must not become an agent map or competing running list.

## Adoption and rollback

Start read-only. Permit opt-in planning writes only after schema, revision-conflict and legacy-adapter proof.
Permit cross-plan coordination only after write-ownership/recovery and integration scenarios pass. Keep existing conversation
records readable and current task-queue ownership intact throughout any record extension.

Production changes follow AGENTS.md: relevant local proofs, independent review, product-map/design/locale updates,
what's-new entries, current landing procedure and required remote confirmation. The approved built-in verifier is
integrated only as its actual stages become available; this proposal does not replace the present proof workflow.

Rollback stops new framework admissions, reconciles active effects, retains evidence and returns to the previous
supported execution path. A newer schema must not be destructively rewritten for an older build. Preserve compatible
readers or export a supported snapshot before opting into a format that the last-good build cannot read.

The owner chooses adoption from the scorecard. A technically passing framework can still fail the clarity or taste
bar; that result is recorded rather than overruled by the framework's author.

## Explicit acceptance gates

The [qualification gates](qualification.md) define domain floors, proposed material gains for light/frontier models,
efficiency, current competitor comparisons and non-code claims. Passing the older software targets alone is insufficient.

## Revision 2 model/workflow comparisons

| Configuration | Question it answers | Attribution rule |
| :--- | :--- | :--- |
| Same light model directly | What the model can do with the ordinary available tools | Disclose its tools/context; do not cripple the baseline |
| Same light model with Flow support, no strong calls | Does context/decomposition/checking help that light-only system? | No hidden frontier planner, judge or repair in the execution path |
| Mixed Flow | Does selective strong guidance/review justify its complete cost? | Attribute gains to the whole mixed system, not to the light model alone |
| Strongest eligible model directly | Is orchestration better than simply using the stronger model? | Same permissions/tools and matched resource comparisons |
| Frontier-led Flow | Does evidence gathering, tooling, alternatives or independent checking improve frontier work? | Count all support and integration, including failed candidates |

A blinded evaluation judge is outside all execution paths equally and is costed separately. Its presence does
not turn a light-only execution into mixed assistance; any feedback used to repair that execution does.
Human preference and factual validity have different labels. Report evaluator disagreement and audit some judgments
against external evidence/human assessment. An eloquent answer cannot win over a violated hard requirement.

Start from the existing rotating roughly-eight-request pilot with representative non-code work, then the roughly
forty-request full set. Stratify by domain, difficulty, checkability, dependency structure, language and available
account configuration. Do not run the entire Cartesian product of paths blindly; use staged paired comparisons
inside the approved evaluation budget. A pilot discovers failures, not universal superiority.

## Ablations and uncertainty

On representative cases, remove one support at a time: retrieved context, examples, decomposition, strong guidance,
independent collection, alternative candidates and review reuse. Compare accepted quality, full cost, latency and
human interventions. Retain required protection even if a small sample does not reveal its safety benefit.

Keep held-out requests and failure fixtures out of examples/route tuning. Counterbalance candidate order and hide
model/vendor identity from preference judges. Repeat variable cases and report paired uncertainty; do not tune on
the reported qualification set. Cold/warm/resumed caches and model/tool versions remain separate conditions.
Use per-domain quality floors and predeclared material-regression criteria. Reject a policy that buys average
improvement by sacrificing a hard constraint, a weakly checkable family or a critical safety outcome.

Report accepted-work cost, critical error rate, requirement coverage, source accuracy, artifact usability, correction
rate, intervention time, route overhead, escalation rate and p50/p95 latency. Unknown token/cost accounting blocks
a definitive economy-win claim. Model self-confidence is not calibrated correctness. A user's next message is not
proof that an answer was factually right. Current Auto preference data must retain that distinction.

If evidence supports only light-model extraction or coding gains, say so. To claim gains for frontier models or
other domains, qualify those strata separately. No finite benchmark establishes being best at everything forever.
Keep the measured quality/cost frontier rather than force one aggregate winner when routes have real trade-offs.

## Complaint-derived regression pack

These cases are independent Faber requirements, not reproductions of competitor code. References resolve to the
qualified report classes in [BMAD](research-bmad.md) and [GSD](research-gsd.md).

| Audit references | Fixture | Required result |
| :--- | :--- | :--- |
| B01/B02/G02 | One trivial task, large unrelated catalog/history | No full-method ceremony or irrelevant context dump |
| B03/B09/G01 | Hidden dependency and explicit negative requirements | Raise rigor when needed; preserve every mandatory constraint |
| B04/B05/G09/G10 | Resume checked work amid unrelated edits and another copy | Revalidate only applicable inputs; attribute output to its actual task/root |
| B06 | Same candidate reviewed with overlapping coverage | Reuse valid dimensions; preserve required independence and dispositions |
| B07/G03/G04 | Reordered milestones, inserted tasks and archived sections | Select an authorized ready outcome; do not invent completion from position |
| B08/G06/G07 | Background worker lacks nested-agent or isolation capability | Visible failure/fallback that preserves required review; no fake execution |
| B10/B12 | Non-code task with code-specific examples available | Use the matching domain contract, not irrelevant software ceremony |
| B11/G14 | Unsupported model/effort, locked files, accents and spaces | Clear incompatibility/recovery, no silent model substitution or success |
| G05 | Requirement, artifact, checker or relevant source changes after pass | Applicable evidence becomes stale; retain historical receipt |
| G08 | Worker exits cleanly without required output; repeated identical inputs | Bounded persistent non-progress response, no reset-by-restart retry loop |
| G11 | Months of logs and repeated migrations | Bounded disposable state; preserve original content and acceptance summaries |
| G12 | Role default conflicts with user's allowed quality/model policy | User policy wins; selected model/effort/reason is recorded |
| G13 | Valid output produced outside the nominal dispatch path | Reconcile safely and check missing evidence without forged provenance |

Also plant light-model fluent errors, a wrong strong-model plan, false tool results, shared false assumptions across
several agents, biased candidate order, degraded refinement and a media file never visually inspected. The system
must reject unsupported acceptance and retain the previous valid candidate when an attempted refinement is worse.

## Rollout condition

The approved R435 sequence remains binding. Add optional record fields/read-only observation first. Qualify direct
and grounded routes before guided execution or portfolio search. Run new routing in comparison/shadow mode where
possible; shadow model calls still require budget and must not duplicate external effects. Keep the previous
qualified path available. Person-approved memory and permissions cannot be changed by a learned routing score.

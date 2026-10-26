# Independent plan review and delivery-to-plan checks

Status: Planned for Phase 4 at the owner's request; not implemented. Technical design proposed by AI, not yet
approved or measured. This extends R146/R147 and the existing verifier direction (R190); it does not replace them.

## Request and intended result

Owner request, preserved verbatim:

> Coloque no plano/roadmap para implementar isso do melhor jeito possivel

The preceding discussion concerns the Elephant-Goldfish Model (EGM): learn the relevant project context, preserve
the intended behavior, independently review the plan before substantial implementation, and compare the delivered
behavior with the agreed requirements. The owner requested scheduling this work, not installing the plugin or
approving every implementation choice. Code must still satisfy the project's checks and independent code review.

Success means fewer missed requirements and less rework per accepted task, with the complete review cost counted.
No quality, time or token saving is assumed. A clearer document alone is not acceptance evidence.

## Placement and prerequisites

- Phase 3 supplies the general project conversation, helper/toolbox and durable project-state foundations; extend
  those as they become available. Do not delay unrelated Phase 1a or Full Chat work to build this proposal.
- Phase 4 starts with one end-to-end Faber pilot, after the verifier and the required native role controls are
  available and qualified. Broader adoption depends on the pilot's evidence, not on the date or plugin installation.
- Existing helpers launch fresh sessions but inherit the caller's permissions, return free-text reports and require
  the other vendor (`engine/src/helper.rs`, `engine/src/engine/helpers.rs`). They do not yet provide this protocol.
- Existing mission contracts and the exact-candidate review queue provide reusable validation patterns, not a ready
  conversation workflow. The coordinator is limited to one task and does not have a repair loop. Its current queue
  rejects same-vendor review. Do not assume a planned fallback or durable helper recovery already exists.

## Delivery sequence

### 1. Establish the relevant project knowledge

- Reuse project instructions, living documentation and the code-derived map before generating new descriptions.
  Keep extracted facts separate from AI explanations, owner decisions and proposed rules.
- Build a small source index, then inspect only the affected areas and their callers. Let reviewers search beyond
  the author's chosen files within the approved reading boundary and resource budget. Record additional sources.
- Give each explanation its sources and revision; invalidate affected descriptions when their inputs change.
  Unknown, missing or partially inspected areas stay explicitly incomplete. Do not require a README per directory
  or an AI scan of the entire repository before useful work. Existing human-written documentation is preserved.
- Use current approved documents as requirements. Dated proposals and inferred behavior are context, not new
  authority. An inferred rule never silently becomes an owner decision.

### 2. Run a bounded independent plan review

- Preserve the person's requirements, constraints and decisions verbatim. Summaries are navigation aids, never
  replacements for the originals. The short plan records outcomes, examples, non-goals, reuse, affected boundaries,
  dependencies, assumptions, unresolved choices and the checks that would demonstrate success.
- Extend helper dispatch with an engine-controlled review purpose and structured findings. Preserve ordinary helpers
  that legitimately write code. Reuse the existing session launch, quota accounting and activity display.
- A reviewer starts without the author's transcript or private reasoning. It receives the exact requirements,
  approved decisions, immutable plan and source snapshot, and project rules. It must not retrieve the author's
  conversation or memory to fill gaps. Fresh context alone does not isolate files or configured memory services.
- Prove read-only controls on each supported CLI/platform before using that combination for review. Refuse writes,
  privilege escalation and tools that could mutate the project; do not rely on an instruction to behave. Keep
  credential boundaries and native sign-in unchanged. Unsupported controls mean unavailable, not a weaker pass.
- The first slice uses the other vendor. If unavailable, preserve the work and report review unavailable under the
  existing quota policy. A fresh same-vendor fallback requires a separate, qualified policy change consistent with
  R146, including the final queue; never weaken today's queue silently.
- One review combines comprehension, requirement coverage, reuse, feasibility and edge cases. Small low-risk changes
  skip design review and keep their existing checks/review rules. Cross-area or costly changes use it; security and
  data-loss risks add a second independent focused reviewer. Risk escalation follows observable affected boundaries
  and existing project rules, not only a label chosen by the author. Run independent reviewers in parallel only
  against the same frozen input; later stages use the revised input, not an unfinished earlier stage.
- The initial independent pass does not see another reviewer's findings. Subsequent repair passes receive resolved
  findings and their evidence so they do not invent the same issue again. Apply R147: an empty review is valid;
  optional preferences do not block; blockers need a concrete violated requirement, scenario or demonstrable defect.
- Proposed starting limit: one review, one correction and one targeted re-review, inside the existing task budget.
  The limit is unmeasured and must be evaluated. Reaching it does not accept residual blockers: park the affected
  work, retain the reason and continue only independent work. Ask the owner only for choices reserved to them.

### 3. Bind execution and delivery to the agreement

- The engine records a versioned review request before dispatch: requirements/decision references, plan digest,
  source snapshot and coverage, model/session, launch policy, attempts, limits and known or unknown usage. Include
  relevant untracked/local edits and source discovery; a contract digest alone does not fingerprint source content.
- Bind review results to those exact inputs. Additional source reads become part of the record. Changed relevant
  sources, requirements, decisions or check definitions invalidate affected evidence. Resolve source reads from a
  stable snapshot and detect changes before acceptance, including dependencies outside the initially selected files.
- The engine validates the structured result and stores its receipt; free-text success, a launched reviewer or the
  author's claim is never approval. Failed, cancelled, malformed, missing and incomplete reviews remain distinct
  from pass. After a crash, a saved request may start a fresh review; a partial report never becomes a completed one.
- Keep plan readiness separate from implementation acceptance. Within this workflow, execution uses the reviewed
  agreement; changing it triggers affected review again. Full access cannot promise a global pre-edit filesystem
  barrier. Enforce the validity of the workflow's evidence without silently changing the person's permission mode.
- After implementation, compare the candidate against each original requirement and approved change: satisfied with
  evidence, unsatisfied, or not verified. Use program checks for observable facts and independent review for meaning;
  an AI comparison report is not executable proof. Final checks and code review bind to the exact candidate.
- Classify drift as an implementation defect, stale documentation, an unimplemented proposal or an authorized change.
  Never rewrite a requirement to excuse a defect. Update living descriptions after verified authorized changes;
  preserve the original request and decision history. Recheck the changed area and affected dependents only.

## Experience, limits and cost

The person stays in the existing Projects conversation. Use its activity rows, short status and folded evidence;
do not restore the removed mission board or add an Elephant/Goldfish mode, mandatory setup interview or settings
without a demonstrated need. Direction, taste, money and contradictions of owner decisions remain owner choices;
reviewers and authors cannot approve them. The workflow adds no authority to send, publish, pay or delete.

Project knowledge and review state are durable, bounded and loaded on demand. Close reviewer processes after work;
do not add an idle daemon or periodic whole-project scan. Respect existing live-worker, daily, task and memory caps;
retain unknown usage as unknown. Compare total input/output/cache use and time per accepted task, including failed
reviews, retries and source discovery. Keep existing memory limits and measure CPU/RAM during the pilot and at idle.

## Acceptance and rollout

- [ ] Replayed checks reject stale plans/sources/decisions, malformed or absent findings, forged author approvals,
  missing required reading, exhausted limits and partial reports. A relevant untracked edit invalidates evidence.
- [ ] A source omitted by the author can be discovered by the reviewer and affects coverage and invalidation.
  Inaccessible or budget-limited sources produce an incomplete result, not a clean review.
- [ ] Native runs on each enabled CLI/platform demonstrate no inherited parent-only marker, refused project writes
  and escalation, and no history/memory tool access. Inspect actual child results and launch controls; fixture-only
  evidence or the reviewer saying it was isolated is insufficient.
- [ ] Interruption/restart preserves the request and reason, closes abandoned processes and restarts safely without
  accepting partial work or repeating authorized external effects. Quota exhaustion never spends paid credits alone.
- [ ] End-to-end cases catch an omitted user requirement, a stale plan, an unresolved blocker and a working-looking
  implementation that violates an agreed behavior. A small correction avoids the design-review overhead.
- [ ] Run one medium Faber change through the complete slice, including final checks and independent code review.
  Then compare a budgeted representative sample with the current workflow using matched model versions and task
  difficulty, including small tasks and cases where design review should help. Use blind independent evaluation and
  executable evidence; count rework, missed requirements, owner interventions, total tokens, time, CPU and memory.
- [ ] Publish the measurements and an independent review before widening the default. A one-task pilot establishes
  feasibility, not general savings. If benefits do not justify the cost, narrow activation or revise the approach.
  Update the product map, design guidance and locales only as corresponding behavior is actually implemented.

## Sources and deliberate exclusions

This is an original Faber adaptation, not a bundled copy of the EGM plugin. Its source remains an optional catalog
candidate when that capability exists; implementation does not require installing it.

- [Google announcement](https://opensource.googleblog.com/2026/10/introducing-elephant-goldfish-model-plugin.html).
- [EGM review](https://github.com/google/elephant-goldfish-model/blob/main/skills/egm_review/SKILL.md): five review
  stages, rather than five identical rounds. Do not inherit automatic acceptance of residual findings at a cap.
- [Bootstrap](https://github.com/google/elephant-goldfish-model/blob/main/skills/egm_bootstrap/SKILL.md): useful
  architectural summaries, but sampled reads and omitted tests cannot establish complete business rules.
- [Drift report](https://github.com/google/elephant-goldfish-model/blob/main/skills/egm_drift_report/SKILL.md) and
  [fresh-context limits](https://github.com/google/elephant-goldfish-model/blob/main/docs/codex.md).

No mandatory full-tree documentation, unlimited reviewer swarm, fixed five-stage pipeline for every task, new
runtime dependency, performance promise or automatic approval of unresolved product choices is part of this plan.

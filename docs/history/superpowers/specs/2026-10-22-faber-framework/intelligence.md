# Adaptive work: light models, frontier models and collaboration

Status: Flow revision 2 proposal. This is a runtime design, not measured capability or implemented routing.
Basis: [primary research](research-methods.md), [BMAD cases](research-bmad.md) and [GSD cases](research-gsd.md).

## The objective

Increase correctly completed useful work, preserving the person's intent, with lower total waste. Improve the
system around each model rather than claim to change its intrinsic intelligence. Light and frontier models both
need good information, suitable tools and feedback, but do not need identical workflows.

Only models and modalities actually offered by the person's connected, permitted official CLIs are eligible.
Haiku, Flash and Luna in the request illustrate capability classes; no new provider, model ID, API, local model
or training pipeline is added by this proposal. A single-vendor or light-only account remains usable within its
qualified envelope; an unavailable strong model is not replaced with a false claim of equivalent quality.

## Five possible paths, not five mandatory stages

| Path | Use when | Execution | Avoid when |
| :--- | :--- | :--- | :--- |
| Direct | Short, familiar task; qualified model or native tool suffices | Current conversation/tool solves it with relevant checking | Hidden dependencies, weak checking or high-impact uncertainty |
| Grounded steps | Missing facts, computation, observation or sequential dependency | One worker obtains evidence, resolves bounded steps, inspects output | Steps do not add information or eliminate a failure mode |
| Guided execution | Complex decomposition but constrained, separable implementation | Qualified strong session prepares a short checked plan; light workers execute suitable units | The supposed small units each require the original hard reasoning |
| Independent alternatives | Genuine choice/search with meaningful comparison; broad separable research | Bounded independent candidates or evidence searches; integrate and check | Shared irreversible actions, highly sequential tasks, unreliable evaluator |
| Strong direct / escalation | Hard, novel, poorly checkable or failed supported work | Qualified stronger session handles the hard part or takes over with a bounded handoff | Extra strength does not address a broken tool, missing authority or absent evidence |

The same task can change path when evidence changes. A frontier-led task may delegate clerical work to light
sessions. A light-led task may request one strong decision and continue. No permanent model-to-role hierarchy.
If a tool can perform the task deterministically, do not pay a model to imitate it.

## What a light worker receives

Compile a task packet from existing saved plans, authorized source material, current memory and kit selection.
This is a bounded derived tool result or handoff, not another editable plan or system-prompt expansion:

- Exact required wording and source references, desired output, exclusions and constraints that must stay true.
- The smallest useful inputs, relevant current facts and their provenance/freshness; explicit unresolved unknowns.
- One applicable worked example or accepted pattern when it reduces a known failure; examples are not new rules.
- Allowed scope, output shape and concrete acceptance checks, including a meaningful counterexample where suitable.
- Checked dependency outputs, current candidate/version and an explicit condition for asking for help or stopping.

Completeness comes before brevity. Split required context that does not fit; never drop mandatory requirements
to reach a compression target. The existing approved 6 KB start card remains unchanged. Details travel through
tools and normal messages; model/effort/tools/system prompt do not churn inside a running conversation.

Useful support removes a real difficulty: source extraction before a report, checked calculations before prose,
a layout example before a component, a timing sheet before video edits. The worker still must produce the actual
output and pass the applicable checks. An example cannot supply knowledge or a sensory modality it lacks.

## Improve frontier work too

Use frontier capability for uncertain decomposition, important trade-offs, deep synthesis and difficult repairs.
Support it with parallel independent evidence gathering when suitable, exact source excerpts, native calculation,
real artifact inspection and counterexamples. Keep one accountable synthesis of the whole requirement.

For a valuable ambiguous design, make a small number of distinct reversible drafts, compare blind against a
declared rubric and retain an option to reject all. Do not create several near-identical agents to manufacture
agreement. Do not show a proposed answer to an independent researcher before they collect their own evidence.
After aggregation, check cross-part consistency and lost requirements, not just each part separately.

## Route by demonstrated suitability

Extend Auto's anonymous task classification, quality floor and quota-aware selection. The selection inputs are:

| Dimension | Examples of observable input |
| :--- | :--- |
| Required capability | Current information, tool use, code, numeracy, source comparison, vision, audio or rendering |
| Difficulty and novelty | Number of dependent decisions, unseen interfaces, incomplete evidence, conflicting constraints |
| Checkability | External check coverage, human inspectability, quality of the evaluator and cost of a missed error |
| Risk and reversibility | Data, permissions, reserved effects and whether a trial can safely be discarded |
| Decomposability | Independent outputs, shared mutable state, integration cost and sequential dependencies |
| Resources | Authorized quota/budget, actual tools, cache state, startup cost, RAM/CPU and human attention |

First reject paths that lack permissions, required modalities, original constraints or mandatory safety/review.
Among qualified paths, prefer the measured quality/cost frontier, giving quality precedence. Do not substitute
a cheap trial for a strong initial route on tasks where it predictably fails or cannot be checked adequately.
If no route meets the quality floor, report the missing capability/evidence or ask the material question.

Cold start uses current conservative Auto defaults and verified capability probes. Observed task outcomes can
later refine route suitability with uncertainty and minimum-sample safeguards; do not invent a probability from
one success or the model's confidence. Before enough data, prefer the known qualified path, not speculative learning.
Expose the reason in current status/technical detail, without asking the person to configure an agent organization.

## Escalation decision

Before another model call, identify the bottleneck:

1. Missing facts: search/read/observe a suitable source. Missing tool/auth: report that condition; another model
   cannot invent access. Broken environment: diagnose it rather than cycle through stronger models.
2. Wrong implementation with a good criterion: repair from the concrete failure using the existing bounded loop.
3. Bad decomposition or hard reasoning: request a stronger independent decision, amend the scoped plan, then rerun
   affected checks. Requirements or direction changes still follow the person's authority.
4. Unresolved disagreement without a trustworthy evaluator: obtain new evidence or surface the choice; never
   certify correctness by majority vote, persuasive explanation or a weak model's approval alone.

Any stronger model/effort runs in an authorized new helper, conversation or existing retry branch, preserving the
originating session's choice and cache rules. The current helper has its own fixed qualification/defaults: optional
per-task selection requires an explicit backward-compatible extension and probes before it is advertised as available.
No prompt can bypass the engine's model, tool, quota or process limits.

## Spending control

Estimate the complete path: classification + briefing + startup/context + execution + tools + review + integration
+ expected repairs. Report actual usage with cache components; unknown values stay unknown. API-equivalent prices
are accounting units, not a fabricated subscription bill. Respect shared-account pacing and existing hard limits.
Reserve sufficient budget for required checks and review before starting discretionary candidates. Stopping new
admission cannot cancel or duplicate an effect already in flight. Existing 150%-estimate economy behavior remains.

Buy extra computation only for a named uncertainty with a plausible quality benefit: another independent source,
a hard intermediate decision, a diverse candidate or a required review. A second pass that merely restates the
first is waste. Missing required quality is never solved by silently lowering the acceptance bar under budget pressure.

## Learn without inventing memory or authority

Extend existing local Auto evidence with distinct events for user preference, executed validity, independent
judgment and later detected failure. The user's next message remains the current weak acceptance proxy; it cannot
be relabeled as factual truth. Keep model/tool version, task family, path, cost completeness and outcome provenance.

Use bounded rolling statistics, uncertainty and drift invalidation. New model/tool versions require conservative
qualification and held-out checks. No always-running training process, online self-modification or rule promotion.
Approved examples come from existing authorized artifacts/skills; lasting lessons/taste follow R435's person-approved
writers. Do not create a new `.faber/recipes` or model-training library. Never learn from credential stores.

## Failure containment and acceptance

The engine owns attempt identity, attribution, budgets, effect reconciliation and evidence references. Persist
non-progress state across restarts; changed wording alone is not progress. Reuse the current repair budget rather
than multiply retry loops at every layer. A missing summary becomes a visible incomplete result, not infinite work.

Required evidence binds requirement revision, input/output content, checker/rubric version and independence.
Reuse a valid review only for covered unchanged inputs with recorded dispositions; security's two reviews remain
separate. If an outside worker produced valid work, reconcile identity and rerun missing applicable checks without
forging its execution or reflexively rebuilding it. Ambiguous external effects require reconciliation first.

Report separately: artifact produced, applicable machine checks, independent judgment, person's taste approval,
destination integration and publication. An output file, a successful export or a fluent report alone proves none
of the other layers. Missing modality, evidence or reviewer stays explicit and cannot count as completed quality.

If a required independent reviewer cannot run, the outcome is not reviewed and cannot be accepted under that
contract, even on a light-only account. When independent review is not required, applicable objective checks can
establish only the criteria they actually cover; record the absence of independent judgment rather than implying
it happened. A route must still satisfy every required domain criterion before the outcome is accepted.

## Qualification

Compare each path with the same model alone and with the strongest eligible direct path under matched budgets.
Test light-only, mixed and frontier-led configurations separately. Attribute gains to the full system and disclose
strong assistance. Ablate context, tools, decomposition, guidance, alternatives and review independently. Keep only
stages that improve accepted results or satisfy mandatory protection. See [evaluation.md](evaluation.md).

# Evidence about improving effective model performance

Status: Research basis, not measured Faber results. Primary sources inspected on 2026-10-22.

## What this can and cannot establish

A workflow can supply information, tools, decomposition, examples and feedback a model would not otherwise use.
It cannot increase a model's learned parameters, grant an absent modality or guarantee expertise in every field.
Improving a system using a light model is different from proving the light model alone matches a frontier model.
No specific performance is assumed for model names in the user's examples, and no new provider is authorized.

The papers below test particular models and tasks, often older ones. Their results motivate experiments; none is a
forecast of Faber savings, reliability or current-model intelligence. No numerical benchmark gain is imported as
a product promise. Pin paper versions in an implementation measurement plan because abstracts can be revised.

| Source | Supported design idea | Important limit |
| :--- | :--- | :--- |
| [Least-to-Most, 2022/2023](https://arxiv.org/abs/2205.10625) | Decompose difficult work into supported subproblems and carry checked intermediate results | Selected reasoning benchmarks do not prove that arbitrary decomposition helps; preserve global constraints |
| [ReAct, 2022/2023](https://arxiv.org/abs/2210.03629) | Alternate task decisions with observable tools and evidence | Tool observations can be wrong, incomplete or hostile; mere tool use does not establish correctness |
| [FrugalGPT, 2023](https://arxiv.org/abs/2305.05176) | Heterogeneous cascades can improve a cost/quality frontier in a calibrated workload | Old API prices and task distribution do not transfer to subscription quotas or all models |
| [Tree of Thoughts, 2023](https://arxiv.org/abs/2305.10601) | Limited alternative candidates and search can help checkable tasks | Search consumes budget and depends on a useful evaluator; do not branch irreversible effects |
| [Intrinsic self-correction study, ICLR 2024](https://arxiv.org/abs/2310.01798) | Prefer external feedback over repeated unsupported self-critique | Tested reasoning cases do not imply all revision is useless; preserve improvements that an appropriate check confirms |
| [Self-Refine, 2023](https://arxiv.org/abs/2303.17651) | Feedback/refinement can improve selected text tasks | Positive task-specific results coexist with self-correction failures; fluency can polish a false answer |
| [Judging the Judges, current revision](https://arxiv.org/abs/2406.07791) | Blind identities, counterbalance order and separate factual checks from preference scores | Model judges have position/quality-gap biases and can share the writer's error |
| [MAST, 2025](https://arxiv.org/abs/2503.13657) | Diagnose system design, inter-agent alignment and termination/verification failures | A taxonomy is not evidence that a proposed new orchestrator has solved those failures |
| [Scaling agent systems, Google Research, 2026](https://research.google/blog/towards-a-science-of-scaling-agent-systems-when-and-why-agent-systems-work/) | Match coordination to dependency structure; sequential work can degrade under multi-agent overhead | Their tasks and architecture experiments do not prescribe one universal routing policy |
| [Anthropic multi-agent research system, 2025](https://www.anthropic.com/research/multi-agent-research-system) | Independent evidence gathering can help broad research and isolate context | Research findings do not imply all domains justify extra agents or their additional token cost |
| [Building effective agents, 2024](https://www.anthropic.com/research/building-effective-agents) | Start with composable simple paths, add orchestration only where needed | The page itself warns that tooling has evolved; apply principles, not historical integration details |

## Design deductions to test

1. The best first action can be a tool, a lookup, a model answer, or a question. Do not force all requests through a
   planning model, then a worker, then three critics.
2. Choose support by the missing capability: evidence for a knowledge gap, a calculator for arithmetic, actual
   rendering for layout, a strong reasoner for a difficult decision, independent research for uncertain coverage.
3. Cheap-first is appropriate only when that route is qualified for the task and reliable checking is affordable.
   Difficult or weakly checkable work may be cheaper and better by starting with a stronger model.
4. Strong planning plus a light worker can work for separable, constrained tasks. The planner must preserve global
   requirements and check critical intermediate results. Bad plans can propagate confidently through every worker.
5. Frontier models still benefit from trustworthy sources, suitable tools, independent counterexamples and actual
   output inspection. Their main advantage is not an excuse to skip checking or perform every clerical step themselves.
6. A majority vote is not evidence when models share assumptions. Multiple candidates need a meaningful comparison
   and an option to reject all of them. A weak judge cannot certify every strong-model answer.
7. Use accumulated outcome evidence to qualify routes by task family and model/tool version; self-confidence and
   the user's next message are insufficient to label factual correctness.
8. Preserve a single-agent baseline. Remove a stage when it adds cost without the promised outcome improvement.

These deductions define Flow experiments in [intelligence.md](intelligence.md) and [evaluation.md](evaluation.md).
They are not claims of unique invention; existing frameworks already implement subsets of these techniques.

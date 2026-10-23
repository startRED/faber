# Company and community practice: revision 3

Status: Primary-source research inspected on 2026-10-22, not a benchmark or adopted implementation.
This extends the [BMAD](research-bmad.md), [GSD](research-gsd.md) and [research](research-methods.md) audits.
The sample covers relevant accessible sources, not every complaint, social post or internal company practice.
Company reports describe their own environments; their gains cannot be transferred to Faber as predictions.

## Correcting the supplied AI answer

| Supplied claim | Finding | Consequence for Faber |
| :--- | :--- | :--- |
| CLAUDE.md Operating Manual | Useful metaphor, not a special enforcement mechanism; [memory documentation](https://code.claude.com/docs/en/memory) describes instructions | Preserve approved scoped instructions; actual permissions belong to the engine |
| Plan-then-build, `/go`, tasks over one hour | Planning is documented; that command and universal threshold were not established in [common workflows](https://code.claude.com/docs/en/common-workflows) | Choose planning by ambiguity, impact and dependency, not a stopwatch |
| TDD red/green for everything | Valuable for checkable software behavior, not sufficient for every artifact | Use domain-specific evidence; research needs sources and a slide needs rendering |
| Parallel worktrees | Supported isolation for repository work; [agent teams](https://code.claude.com/docs/en/agent-teams) have coordination and token costs | Parallelize independent work; isolation does not remove semantic dependencies |
| Three fundamental Anthropic patterns | The [effective-agents article](https://www.anthropic.com/research/building-effective-agents) presents five workflow patterns, not that exact taxonomy | Compose a small set of mechanisms rather than adopting a mislabeled standard |
| Cowork always uses a local VM | Current [Cowork documentation](https://support.claude.com/en/articles/15520349-use-claude-cowork-on-web-desktop-and-mobile) distinguishes surfaces and environments | Do not promise another product's execution environment or entitlements |
| Skills automatically become webhook routines | [Academy](https://academy.claude.com/tutorials/customize-claude-cowork) describes reusable skills and separately scheduled tasks | Reuse and scheduling are separate capabilities with separate authority |
| SuperClaude | A real [community framework](https://github.com/SuperClaude-Org/SuperClaude_Framework), not an official quality guarantee | Study its discoverability and scoped procedures; qualify benefits locally |

The original wording is retained in [request.md](request.md). Corrections above do not silently edit it.

## Company sources and decisions they inform

| Source | Supported observation | Faber deduction / limit |
| :--- | :--- | :--- |
| [Anthropic: code review](https://claude.dev/blog/solving-code-review-at-anthropic/) | Layered checks, fresh review, local review guidance and risk-sensitive human attention; incidents and reversions matter | Verify before review, calibrate findings, measure total intervention burden. Their internal approval model does not override Faber's owner authority |
| [Anthropic: dynamic workflows](https://claude.dev/blog/a-harness-for-every-task-dynamic-workflows-in-claude-code/) | Task-specific compositions address drift and incomplete work through focused contexts | Permit constrained composition, not model-written execution authority. Product-specific APIs are not assumed available in Faber |
| [Anthropic: how skills are used](https://claude.dev/blog/lessons-from-building-claude-code-how-we-use-skills/) | Reusable task procedures load when needed; skill design matters beyond a persona name | Build narrow evaluated procedures through existing kits/catalog; avoid a permanent department hierarchy |
| [Claude Academy: customize Cowork](https://academy.claude.com/tutorials/customize-claude-cowork) | Standing instructions, relevant skills and connected tools have different jobs | Keep policy, procedure and live evidence separate. Scheduling is a later explicit feature, not automatic promotion |
| [Claude Code: best practices](https://code.claude.com/docs/en/best-practices) | Relevant context and a way to verify outcomes are central practical advice | Provide the actual source/tool/check needed; instructions alone do not establish compliance |
| [OpenAI: long-horizon work](https://developers.openai.com/blog/run-long-horizon-tasks-with-codex) | Persistent external state and iterative verification support extended tasks | Resume from current artifacts and receipts; a reported experiment is not a duration guarantee |
| [OpenAI: iterative development](https://developers.openai.com/cookbook/examples/codex/iterating-development-workflows-with-codex) | Goals, plans, observations and execution records support iteration | Adopt these functions in native records, not extra copies of suggested cookbook files |
| [OpenAI: goals](https://developers.openai.com/cookbook/examples/codex/using_goals_in_codex) | Persistent objectives suit uncertain routes with checkable stopping conditions | Extend existing Faber goals; distinguish goal persistence from permission or correctness |
| [OpenAI: skills and prompts](https://developers.openai.com/blog/rethinking-skills-and-prompts-for-gpt-6-astra) | Older scaffolding can become unnecessary as model capabilities improve | Evaluate minimal steering for strong models and more explicit support for weaker ones; remove unhelpful stages |
| [ChatGPT: build skills](https://learn.chatgpt.com/docs/build-skills) | Reusable procedures should be scoped and discoverable | Evaluate trigger behavior and outcomes, not merely whether a skill file loads |
| [ChatGPT: work](https://learn.chatgpt.com/docs/get-started-with-work) | Useful work spans research, files and usable outputs | Qualify full artifact workflows. Hosted features do not imply CLI/subscription availability |
| [OpenAI: review rules](https://developers.openai.com/blog/custom-code-review-rules-for-codex) | Local review rules can express recurring judgment | Test each rule on a real violation, safe contrast and unrelated change before retaining it |
| [OpenAI: trace grading](https://developers.openai.com/api/docs/guides/trace-grading) | Evaluating a path can reveal failures missed by an output-only score | Use observable tool/evidence events; neither import a hosted dependency nor collect private reasoning |
| [Google Research: agent scaling](https://research.google/blog/towards-a-science-of-scaling-agent-systems-when-and-why-agent-systems-work/) | Coordination benefits depend on the task and architecture | Keep sequential tasks coherent and account for communication costs; no universal swarm default |

## Framework mechanisms worth studying

These are source-level mechanisms and documentation, not claims that the complete products beat each other.
No third-party framework, runtime, skill or code is bundled or copied by this proposal.

| Primary source | Mechanism | Adapt / avoid |
| :--- | :--- | :--- |
| [Google ADK graph workflows](https://adk.dev/graphs/) and [older sequential-agent documentation](https://github.com/google/adk-docs/blob/main/docs/agents/workflow-agents/sequential-agents.md) | Current graph composition supports typed data flow; the older templated page flags the ADK 2.0 transition | Use explicit dependencies where order matters; do not recommend a superseded API or adopt an SDK just for a pipeline |
| [Microsoft Agent Framework migration](https://learn.microsoft.com/en-us/agent-framework/migration-guide/) | Documents transition from earlier orchestration libraries | Version-pin lessons and migrations; old AutoGen/Semantic Kernel advice is not automatically current |
| [LangGraph persistence](https://github.com/langchain-ai/docs/blob/main/src/oss/langgraph/persistence.mdx) and [functional execution](https://github.com/langchain-ai/docs/blob/main/src/oss/langgraph/functional-api.mdx) | Checkpoints and replay require careful side-effect boundaries | Reconcile ambiguous effects; do not promise exactly-once external actions from a checkpoint |
| [Temporal AI](https://docs.temporal.io/ai) | Durable execution and recovery around long-lived work | Borrow recovery principles; no permanent workflow server is needed for the proposed first slices |
| [Block Goose recipes](https://github.com/aaif-goose/goose/blob/main/documentation/docs/guides/recipes/recipe-reference.md) | Explicit inputs, instructions and execution configuration | Reuse contract/validation ideas in existing kits; no second recipe folder |
| [GitHub Spec Kit](https://github.github.com/spec-kit/reference/agentic-sdd.html) | Trace intent through specification, planning and tasks | Retain traceability; avoid a mandatory document ceremony for a trivial task |
| [OpenSpec overview](https://github.com/Fission-AI/OpenSpec/blob/main/docs/overview.md) | Distinguishes current truth from proposed change | Display built, approved and proposed separately; keep historical plans from becoming runtime truth |
| [Superpowers](https://github.com/obra/superpowers) | Checkable development practices and behavior-focused skills | Reuse procedural ideas, evaluate actual behavior, avoid unneeded mandatory stages |
| [SuperClaude](https://github.com/SuperClaude-Org/SuperClaude_Framework) | Discoverable commands, modes and specialized instructions | Expose useful capabilities through natural requests; more personas do not prove more intelligence |
| [Cursor rules](https://docs.cursor.com/context/rules-for-ai) and [checkpoints](https://docs.cursor.com/en/agent/chat/checkpoints) | Scoped guidance and recovery affordances | Scope context and clarify recovery boundaries; a code checkpoint cannot undo an email or payment |

## Research conclusions, not company endorsements

The useful common structure is intent, current context, bounded execution, appropriate verification and recovery.
The distinct Faber proposal is to make those compose through its existing plans, Auto, kits, verifier and receipts,
with model-sensitive support and an outcome route the person can understand. [Composition](composition.md) defines it.

No source establishes an absolute best workflow across every domain, model and future version. The honest route
to Erick's requested ambition is broad coverage, explicit limits and continual paired qualification, including
removing workflow stages that do not earn their cost. The [feature backlog](features.md) turns that into proposals.

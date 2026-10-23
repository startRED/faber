# Current baseline and comparison

Status: Proposal research, refreshed against the shared product on 2026-10-22. No installed-app validation.

## Freshness matters

The original conversation copy contained the pre-R215 mission engine and an older roadmap. The shared product had
already removed that engine and approved R435. This revised proposal uses the current shared sources below.
Historical mission contracts are not a reusable current runtime. Do not read, repair or migrate abandoned mission
folders merely to introduce this framework.

Owner decisions constrain design. Source establishes implementation in the inspected revision; proof results
establish tested behavior. Neither a document marked approved nor a source file alone establishes full delivery.

## Existing spine

| Capability | Current source to inspect | Framework use |
| :--- | :--- | :--- |
| Project conversations and separate copies | `docs/design/projects.md`, `engine/src/project/copy.rs` | Keep the current execution unit |
| Saved plans, short status and closing box | `engine/src/engine/plan.rs`, `engine/src/model.rs` | Extend their records rather than create mission state |
| Standing goals | `projects.json`; product map Standing goals | Link long-running outcomes and stop conditions |
| Keep going and started conversations | `docs/design/keep-going.md`, `docs/dev/self-development.md` | Continue and delegate through current mechanisms |
| Verifier and engine checks | `verify` tool; self-development and verifier docs | Bind evidence to the applicable candidate |
| Auto, quotas and spend | product/models; `engine/src/engine/plan.rs` | Reuse routing, usage and over-estimate economy behavior |
| On-demand code map | `map` tool; product-map Project map | Do not generate another prose inventory |
| Memory and approved provenance | `engine/src/project/memory.rs`, `engine/src/project/rules.rs` | Distinguish person-kept items from editable project text |
| Native progress and visual answers | design/running, conversation and visual-answers | Add outcome detail inside current surfaces |
| Development queue and areas | `docs/tasks/`, `.faber-lane`, dev/workflow | Adopt their own format; no user-project claim bureaucracy |

The original copy's empty task folder did not represent the current shared queue. Never infer the whole project's
current plan solely from one isolated conversation copy.

## Approved method to preserve

The shared `docs/superpowers/specs/2026-10-22-own-work-method.md`, pinned by R435, defines the current direction:

- Small / normal / risky work; the contract belongs in the plan, and boundary-crossing raises risk before execution.
- Engine-written receipts tied to file hashes; fresh independent risky-work review; security requires two reviews.
- `.faber/rules.md`, `decisions.md`, `taste.md`, `lessons.md` and `next.md`, with bounded startup context.
- Repository text grants no authority. Approved provenance and adopted-source pointers live in the private data folder.
- Plans stay in conversations, standing goals in `projects.json`, code facts in `map`.
- Adopt a project's existing decision/taste/task sources instead of writing a second copy.
- Legacy plans/recipes/learning/memory remain readable, but new writers do not create them.

The shared index/spec still describe all method slices as unbuilt, while current code/map show slice-one integration.
Treat that as a documentation discrepancy, not proof that every slice has shipped. Verify each integration at its start.
Some product/building prose still refers to missions; current design/projects explicitly records their R215 removal.

## Complete-product coverage

| Area | Framework responsibility | Boundary |
| :--- | :--- | :--- |
| Chat | Answers, sources and optional durable handoff | No mandatory plan for a question |
| Projects | Outcomes across saved conversations, work and delivery | One existing plan/task system |
| Models and subscriptions | Quality floor, quotas, measured total cost | No credentials or unapproved paid fallback |
| Catalog and kits | Tools selected by real need and provenance | No bundled third-party framework |
| Following work | Outcome route, artifact links, current attention | No competing running list or agent map |
| Self-development | Current queue, verifier, review and landing adapters | No resurrected mission machinery |
| Images, research, documents and media | Domain-specific verification and reproducibility | Human taste and missing checks remain explicit |
| PC automation and future phone | Receipts, interruption and authorized effects | Phone is a remote; sending/paying/publishing need authority |
| Privacy, settings and updates | Current trust, rollback and idle budgets | No new settings area or cloud dependency |

## BMAD and GSD

Primary sources inspected on 2026-10-22. Pin actual versions when benchmarking. No code or prompts were copied.
The former GSD repository points to [GSD Core](https://github.com/open-gsd/gsd-core) as its current development home.

| Source | Documented contribution | Faber response |
| :--- | :--- | :--- |
| [BMAD planning](https://docs.bmad-method.org/plan/choose-a-planning-path/) | Clarify intent and size planning to the work | Preserve the approved risk-sized loop |
| [BMAD build](https://docs.bmad-method.org/build/build-a-change/) | Investigate, implement, review and repair | Tie the process to Faber's actual checks and closing records |
| [GSD Core](https://github.com/open-gsd/gsd-core) | Durable phases and fresh-context workers | Preserve continuity while avoiding unnecessary session/brief overhead |

The intended advantage is native integration: subscription pacing, runtime facts, visible output and continuity in
one app. This does not establish that competitors lack equivalent guarantees or that Faber already performs better.

## Revision 2 findings

The refreshed audit includes both GSD Core and GSD Pi, not just the archived original. Pi already documents an
authoritative state engine, derived Markdown, progressive planning, per-unit cost, model profiles, lightweight
models, liveness recovery and a visualizer. BMAD already has selective context and risk-sized planning. These
features cannot be used as invented competitive gaps. See [BMAD](research-bmad.md) and [GSD](research-gsd.md).

The current Faber integration gaps for adaptive work are concrete: plans lack stable outcome/evidence identity;
Auto's learned keep/retry signal does not certify correctness; helper selection currently has fixed qualification
defaults; domain/modal/tool suitability needs measured profiles; cross-plan recovery and exact-outcome invalidation
are new work. Extend these points in existing modules instead of adding a second orchestrator.

The proposed planner/worker/checker responsibilities can use one model, multiple models or deterministic tools.
Their availability must be probed through current official CLIs. Flash is an example in the person's request,
not an authorization to bypass the product's supported-provider scope. See [adaptive work](intelligence.md).

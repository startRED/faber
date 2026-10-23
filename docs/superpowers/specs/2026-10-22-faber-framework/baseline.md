# Baseline and comparison

Status: Proposal research, based on the checked-out repository on 2026-10-22. No installed-app validation.

## Evidence hierarchy

The person's decisions constrain design. Source code establishes what this checkout implements; a documented
vision establishes intended scope. Neither establishes that an installed build has passed an end-to-end check.
When documents disagree, record the conflict instead of silently choosing whichever supports the proposal.

## Existing mechanisms to extend

| Mechanism | Evidence in this checkout | Implication |
| :--- | :--- | :--- |
| Project conversations | `ui/Projects.tsx`, `ui/App.tsx`; product-map Words and Projects surfaces | Preserve conversation-first work |
| Mission coordinator | `engine/src/project/coordinator.rs`, `src-tauri/src/main.rs` (`mission_start`, `drive_mission`) | Extend an executor rather than invent another |
| One task, one writer attempt | `coordinator.rs` (`accept_plan`, module contract) | Multiple tasks and repair are new work |
| Graph, scopes, contracts | `engine/src/project.rs`; design/projects Records | Reuse dependency and overlap validation |
| Durable journal | `engine/src/project/store.rs`, `run.rs` | Private run authority repairs the public mirror |
| Exclusive process-aware leases | `engine/src/project/lease.rs` | Elapsed time alone must not transfer a live worker's rights |
| Exact-result acceptance | `engine/src/project/queue.rs` | A changed candidate needs fresh applicable checks and review |
| Briefs and worker envelopes | `engine/src/project/wire.rs`, `kit.rs`; design/projects Worker wire and Kits | Extend typed contracts and bounded source selection |
| Check and cost records | `engine/src/project/report.rs`, `wire.rs` | Keep unknown cost and checks not run explicit |
| Development task queue | `docs/tasks/README.md`, `scripts/tasks.mjs` | Adapter required; do not maintain a competing backlog |
| Local area claims | `.faber-lane`, `testkit/src/lanes.rs` | Preserve overlap prevention during self-development |
| Model/quota routing | `engine/src/route.rs`, `budget.rs`; product/models | Extend Auto rather than add a framework model selector |
| Helpers, activity and browser | product-map Subagent step, Work log and Page in Faber's browser | Reuse visible activity and intervention |

The task queue has no pending task documents in this checkout, and `.faber/` was absent during the inventory.
That does not imply that mission or project-memory code is absent.

## Planned capabilities, not established as complete

- Multiple-task missions, bounded repair and a durable project-wide outcome roadmap.
- General automatic map extraction, contract enforcement and shared evidence-backed completion for all work.
- Full deliverable library, learned taste and project memory with reusable recipes across domains.
- The built-in verifier beyond its recorded initial measurements: docs/README still says V0 measured, V1 next.
- Shared MCP infrastructure and measured cross-domain benchmarks described by the product vision.

## Documentation conflicts discovered

| Conflict | Current evidence | Planning treatment |
| :--- | :--- | :--- |
| `product/areas.md` says projects have no direct conversation | product-map and `ui/Projects.tsx` show conversations | Preserve the implemented conversation entry |
| `product/following.md` mentions the old mission board | product-map and design/projects say it was removed | Do not describe a board as an existing surface |
| design/projects has an old statement that the coordinator is absent | main.rs starts and drives the coordinator | Use the actual coordinator, retain its one-task limit |
| Product map's unbuilt list includes features named elsewhere as present | Guide and individual surface entries disagree | Verify each integration against code during its slice |

These are findings, not changes made by this proposal. Resolve the relevant conflict before implementing an affected
slice; do not expand this planning task into unrelated product or documentation repairs.

## Full-product coverage

| Faber area | Framework responsibility | Boundary |
| :--- | :--- | :--- |
| Chat | Lightweight answers, sources, optional durable handoff | No mandatory roadmap for a question |
| Projects | Outcomes, dependencies, attempts, deliverables, decisions | Same conversation and permission model |
| Auto and accounts | Existing routing, quotas, actual usage and admission | No credentials or new paid-provider path |
| Catalog and tools | Select by task needs, record versions and provenance | Download from source; never bundle third-party tools |
| Following the work | Project route, activity, source-linked output | No second running list or agent map |
| Verification and self-development | Exact-candidate proof, independent review, existing queue adapters | Respect current landing rules and product phases |
| Images, design and documents | Dimensions, rendering, accessibility where relevant, person's taste | Aesthetic judgment stays distinct from technical validity |
| Video, 3D, mods and reverse engineering | Domain recipes, reproducibility, suitable checks, provenance | Original code; authorized scope and tools |
| Automation and future phone remote | Durable state and interruption | Phone controls work; it does not become another executor |
| Settings, privacy and updates | Existing settings and rollback contracts | No new settings page; local history, telemetry off |

## BMAD and GSD: what to borrow

Sources inspected on 2026-10-22. These are moving projects; pin versions for any benchmark.
The former GSD repository redirects development to [GSD Core](https://github.com/open-gsd/gsd-core).
No code, prompts or third-party assets from either project are included here.

| Source | Documented strength | Faber-specific design response |
| :--- | :--- | :--- |
| [BMAD planning paths](https://docs.bmad-method.org/plan/choose-a-planning-path/) | Intent and planning sized to the work | Keep risk-sized preparation; reuse Faber's product map and decisions |
| [BMAD build](https://docs.bmad-method.org/build/build-a-change/) | Investigate, build, independently review and repair; small changes can take a light path | Do not pretend review or lightweight execution are novel; bind them to Faber's verified candidate |
| [GSD Core](https://github.com/open-gsd/gsd-core) | Durable phases and fresh-context research, planning and execution | Preserve resumability; choose fresh sessions when useful rather than for every tiny step |

The intended difference is a single native product linking runtime authority, human-readable plans, live evidence,
subscription pacing and visible deliverables. This comparison does not establish that competing tools cannot
integrate similar ideas, or that Faber already performs better.

## Why not adopt one unchanged?

Faber already owns process lifetimes, model choice, permission cards, worker reporting and quota accounting.
Layering another orchestration system on top would require reconciling two owners of those facts.
Use the useful methods as design references; implement the smallest Faber-native layer and measure its value.

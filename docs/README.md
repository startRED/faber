# Faber's documents

Which page answers which question, and which pages are history. When a living page and a dated one disagree, the living
page wins; when two living pages disagree, one of them is wrong: fix it in the same commit or name it in the commit body.

## Living pages

They describe Faber as it is now, so a change updates them in its own commit. Each page answers one kind of question
and stays small enough to read whole: read the pages your task names, never a whole folder. `npm run docs` fails when a
page grows past 240 lines or 20,000 bytes, when a page is missing from this table, or when a link or `#anchor` to a page
points nowhere; a page that grows past the limit splits by topic, and its row here says what each part answers.

| Page | Read it when |
| :--- | :--- |
| [../AGENTS.md](../AGENTS.md) | Always: rules for every agent, the order of work, proof commands, commits |
| [product-rules.md](product-rules.md) | Before a change a person sees: where a new thing goes, the taste cards it must not break |
| [product-map.md](product-map.md) | Before a change a person sees, sets or calls: words, screens, settings, stored state, shell commands, features and their checks, known incoherences. Read whole on purpose: it is how a second way to do the same thing is found |
| [decisions.md](decisions.md) | To check a change against the decisions in force (R-numbers). Search it for the words of your change; never read it whole |
| [product/README.md](product/README.md) | Before changing scope: what Faber is, principles, goals, phases and non-goals |
| [product/models.md](product/models.md) | Auto, model choice, or how the CLIs are driven |
| [product/areas.md](product/areas.md) | Chat and Projects, the lead agent, normal and autonomous modes, the toolbox, guardrails |
| [product/building.md](product/building.md) | How Projects builds software: team, workers, self-development, coherence |
| [product/following.md](product/following.md) | Following and watching the agent work, visual answers, learning and memory |
| [product/architecture.md](product/architecture.md) | Processes, memory and CPU budgets, updates, the curated catalog |
| [product/distribution.md](product/distribution.md) | License, provider terms, distribution |
| [product/quota-pacing.md](product/quota-pacing.md) | Auto's quota pacing, quota readings and their measurements |
| [design/README.md](design/README.md) | Before any screen change: the design in short, rules for every screen, the first frame, the screen cards |
| [design/looks.md](design/looks.md) | Themes, presets, colour, type, motion, the one grammar, idle cost, effects |
| [design/brand.md](design/brand.md) | The mark, the app icon, empty states, brand rules |
| [design/layout.md](design/layout.md) | Window widths, titlebar, sidebar, the homes, answers, the work log, subagent steps, the running column |
| [design/settings.md](design/settings.md) | The Settings screen, the first run and the tutorial |
| [design/projects.md](design/projects.md) | The mission engine's published contract: records, worker envelope, kits, proof isolation, commands |
| [dev/testing.md](dev/testing.md) | Tests: `cargo t`, screen tests, build folder and slots, what a run prints |
| [dev/workflow.md](dev/workflow.md) | Landing work: lane claims, merge on green, the task queue, a red base, the numbers given at landing |
| [dev/sentinel.md](dev/sentinel.md) | The UX sentinel and the visual critique |
| [security-r97-review.md](security-r97-review.md) | The verified security review; its R121/R124 section is kept current |
| [tasks/README.md](tasks/README.md) | What a session does next, in order, one file per task (`node scripts/tasks.mjs next`) |
| [brand/](brand/) | Logo sources |

## Dated pages

They record one moment. Never read one to learn how Faber works today: its behaviour, once built, lives in the living
pages above. Only a page in progress is still updated (its boxes and `Status:` line, below); a finished or abandoned
one is never touched again, and its boxes say nothing.

| Page | Status |
| :--- | :--- |
| superpowers/specs/2026-10-11-faber-redesign-design.md | Absorbed into product/ and design/ |
| superpowers/specs/2026-10-11-auto-quota-pacing-design.md, plans/2026-10-11-auto-quota-pacing.md, auto-quota-pacing-report.md | Shipped |
| superpowers/plans/2026-10-11-b2 … b9 (test runner, looks, running engine, layout, tray, first run, Settings, download) | Shipped |
| superpowers/specs/2026-10-12-self-development-mvp-design.md, plans/2026-10-12-self-development-mvp.md | In progress (missions, T13) |
| superpowers/specs/2026-10-12-faber-rework-design.md | Proposal, nothing built |
| superpowers/specs/2026-10-12-generated-pipelines-design.md, 2026-10-12-tool-selection-rework-design.md | Proposals, partly superseded by the rework design |
| superpowers/specs/2026-10-12-ux-sentinel-design.md, plans/2026-10-13-ux-sentinel-s0.md | Slice S0 shipped (dev/sentinel.md); the rest is a proposal |
| superpowers/specs/2026-10-14-test-system-rework-design.md, plans/2026-10-14-test-system-rework.md | In progress |
| superpowers/specs/2026-10-16-browser-preview.md | Proposal, nothing built (recommends yes, after a week of visual critiques) |
| superpowers/plans/2026-10-14-parallel-work-loop.md | The development loop (R128, R129); its rounds table is kept current |
| superpowers/specs/2026-10-16-own-lessons-into-faber.md | Proposal, nothing built (lessons from building Faber by hand, mapped to features for 1a) |
| superpowers/specs/2026-10-17-built-in-verifier.md | Approved (R190), in progress: V0 measured ([research/2026-10-17-verifier-v0-measurements.md](research/2026-10-17-verifier-v0-measurements.md)), V1 next. Faber's own verifier in the engine, checks in seconds on the PC, GitHub only confirms |
| superpowers/specs/2026-10-17-agent-pane-webview-spike.md | Spike done: go for step 7 of "Watching the agent work", with the five rules in its Recommendation |
| research/ | Measurements, probes and reviews of the day in their name; model names and prices in them go stale fast. The UX, notification and screen-flake reviews of 2026-10-12 live here; their conclusions are in design/ and product/ |

A new spec or plan goes to `superpowers/specs` or `superpowers/plans` with the date in its name and a `Status:` line at the
top (proposal, in progress, shipped, superseded by …), and gets a row here. While it is in progress, each landed step
ticks its box and updates the `Status:` line, so a fresh session resumes from the file instead of from memory. When it
ships, its behaviour moves into product-map.md and the design/ or product/ page it belongs to, and its row says shipped.
Default searches skip `superpowers/` and `research/` (`.ignore`); name the path to search them.

# Faber's documents

Which page answers which question, and which pages are history. When a living page and a dated one disagree, the living
page wins; when two living pages disagree, one of them is wrong: fix it in the same commit or name it in the commit body.

## Living pages

They describe Faber as it is now, so a change updates them in its own commit.

| Page | Answers |
| :--- | :--- |
| [../AGENTS.md](../AGENTS.md) | Rules for every agent, the order of work, proof commands, commits |
| [product-rules.md](product-rules.md) | Where a new thing goes, and the taste cards it must not break |
| [decisions.md](decisions.md) | Every decision in force (R-numbers), with its author; a new one takes the next number |
| [product-map.md](product-map.md) | What exists today: words, screens, settings, stored state, shell commands, features and their checks, known incoherences |
| [design.md](design.md) | How Faber looks, moves and lays out each screen; the screen tests and their budgets |
| [roadmap.md](roadmap.md) | Where Faber is going and why: principles, Auto, the two areas, guardrails, architecture, phases |
| [security-r97-review.md](security-r97-review.md) | The verified security review; its R121/R124 section is kept current |
| [brand/](brand/) | Logo sources |

## Dated pages

They record one moment. Never read one to learn how Faber works today: its behaviour, once built, lives in the living
pages above. Only a page in progress is still updated (its boxes and `Status:` line, below); a finished or abandoned
one is never touched again, and its boxes say nothing.

| Page | Status |
| :--- | :--- |
| superpowers/specs/2026-10-11-faber-redesign-design.md | Absorbed into roadmap.md and design.md |
| superpowers/specs/2026-10-11-auto-quota-pacing-design.md, plans/2026-10-11-auto-quota-pacing.md, auto-quota-pacing-report.md | Shipped |
| superpowers/plans/2026-10-11-b2 … b9 (test runner, looks, running engine, layout, tray, first run, Settings, download) | Shipped |
| superpowers/specs/2026-10-12-self-development-mvp-design.md, plans/2026-10-12-self-development-mvp.md | In progress (missions, T13) |
| superpowers/specs/2026-10-12-faber-rework-design.md | Proposal, nothing built |
| superpowers/specs/2026-10-12-generated-pipelines-design.md, 2026-10-12-tool-selection-rework-design.md | Proposals, partly superseded by the rework design |
| superpowers/specs/2026-10-12-ux-sentinel-design.md, plans/2026-10-13-ux-sentinel-s0.md | Slice S0 shipped (design.md, UX sentinel); the rest is a proposal |
| superpowers/specs/2026-10-14-test-system-rework-design.md, plans/2026-10-14-test-system-rework.md | In progress |
| superpowers/plans/2026-10-14-parallel-work-loop.md | The development loop (R128, R129); its rounds table is kept current |
| research/ | Measurements, probes and reviews of the day in their name; model names and prices in them go stale fast |
| ux-review.md, notifications-review.md, screen-test-flakes.md | Reviews of one moment; their conclusions live in design.md and roadmap.md |

A new spec or plan goes to `superpowers/specs` or `superpowers/plans` with the date in its name and a `Status:` line at the
top (proposal, in progress, shipped, superseded by …), and gets a row here. While it is in progress, each landed step
ticks its box and updates the `Status:` line, so a fresh session resumes from the file instead of from memory. When it
ships, its behaviour moves into product-map.md and design.md and its row says shipped. Default searches skip
`superpowers/` and `research/` (`.ignore`); name the path to search them.

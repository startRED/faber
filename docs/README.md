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
| [product/frontend.md](product/frontend.md) | Original frontend design/proof skills, how both AIs receive them, what delivery checks do and do not prove |
| [product/following.md](product/following.md) | Following and watching the agent work, visual answers, learning and memory |
| [product/architecture.md](product/architecture.md) | Processes, memory and CPU budgets, updates, the curated catalog |
| [product/distribution.md](product/distribution.md) | License, provider terms, distribution |
| [product/quota-pacing.md](product/quota-pacing.md) | Auto's quota pacing, quota readings and their measurements |
| [product/parity.md](product/parity.md) | What Claude Code, Codex, claude.ai and ChatGPT have that Faber still lacks, how each gap closes, and what is never closed |
| [design/README.md](design/README.md) | Before any screen change: the design in short, rules for every screen, the first frame, the screen cards |
| [design/looks.md](design/looks.md) | Themes, presets, colour, type, motion, the one grammar, idle cost, effects |
| [design/brand.md](design/brand.md) | The mark, the app icon, empty states, brand rules |
| [design/conversation.md](design/conversation.md) | An open conversation: the answers and their bar, your messages, the viewer, linked research reports and the work log |
| [design/reading.md](design/reading.md) | Reading width, selections, durable scroll position and the latest-message control |
| [design/visual-answers.md](design/visual-answers.md) | Native inline pieces, saved choices, generated HTML/SVG cards and their web-access rules |
| [design/layout.md](design/layout.md) | Window widths, titlebar, the homes, the running column |
| [design/chat-groups.md](design/chat-groups.md) | Chat groups, shared instructions and knowledge files |
| [design/sidebar.md](design/sidebar.md) | Sidebar area lists, pinning, export, archive and footer |
| [design/composer.md](design/composer.md) | The shared message box: file mentions, commands, earlier messages and their search |
| [design/branch.md](design/branch.md) | "Branch from here" and the arrows between the versions of a message |
| [design/queue.md](design/queue.md) | Queued messages, in-place editing, keyboard controls and drafts when a message leaves the queue |
| [design/rewind.md](design/rewind.md) | The message menu's Go back, Redo and recovery confirmation, file preview and saved-message notice |
| [design/visual-answers.md](design/visual-answers.md) | Reply pages, captured file versions, Mermaid, isolation and measured bounds |
| [design/running.md](design/running.md) | The running column: rows, question cards, done cards, when it opens by itself |
| [design/keep-going.md](design/keep-going.md) | Keep going: the chip, the quiet lines, the quota warning, the running row and the summary on coming back |
| [design/search.md](design/search.md) | Sidebar message search, the command box (Ctrl+K) and find in a conversation (Ctrl+F) |
| [design/subagents.md](design/subagents.md) | A reply's subagent and helper rows, their running-column view, the stuck signs and the pages opened in Faber's browser |
| [design/catalog.md](design/catalog.md) | Catalog browsing, installation, on/off, Undo and per-source inventory |
| [design/settings.md](design/settings.md) | The Settings screen |
| [design/first-run.md](design/first-run.md) | The first run and the tutorial |
| [design/projects.md](design/projects.md) | The Projects screens, what a project stores, and how its checks run |
| [dev/testing.md](dev/testing.md) | Tests: `cargo t`, screen tests, build folder and slots, what a run prints |
| [dev/build-folders.md](dev/build-folders.md) | Build-cache cleanup, disk budget and preserving exact proof artifacts |
| [dev/fair-bench.md](dev/fair-bench.md) | The fair benchmark against TL-ADE: the meter, the suite and real-work tiers, the notes |
| [dev/benchmark.md](dev/benchmark.md) | Running the light benchmark: the one command, what a run measures, what it leaves out |
| [dev/workflow.md](dev/workflow.md) | Landing work: lane claims, merge on green, the task queue, a red base, the numbers given at landing |
| [dev/whats-new.md](dev/whats-new.md) | Writing a what's-new entry: the words, the kinds, numbers for speed and memory |
| [dev/sentinel.md](dev/sentinel.md) | The UX sentinel, the visual critique and the first-user review |
| [dev/self-development.md](dev/self-development.md) | Faber building Faber from its own project conversation: how to start it and which part does what |
| [security-r97-review.md](security-r97-review.md) | The verified security review; its R121/R124 section is kept current |
| [tasks/README.md](tasks/README.md) | What a session does next, in order, one file per task (`node scripts/tasks.mjs next`) |
| [brand/](brand/) | Logo sources |

## Dated pages

They record one moment. Never read one to learn how Faber works today: its behaviour, once built, lives in the living
pages above. Only a page in progress is still updated (its boxes and `Status:` line, below); a finished or abandoned
one is never touched again, and its boxes say nothing.

| Page | Status |
| :--- | :--- |
| [superpowers/specs/2026-10-22-faber-framework/README.md](superpowers/specs/2026-10-22-faber-framework/README.md) | Proposal revision 3, nothing built: company/community research, Faber context audit, adaptive composition, twelve capability candidates, BMAD/Core/Pi audit, evidence-backed progress and fair qualification; companion pages linked from its README |
| [superpowers/specs/2026-10-24-cli-tools-live-manifest.md](superpowers/specs/2026-10-24-cli-tools-live-manifest.md) | Offline signature foundation checked; native document validation tracked by the next contract; live publication, compatibility delivery and executable installation remain unbuilt |
| [superpowers/specs/2026-10-24-live-manifest-policy.md](superpowers/specs/2026-10-24-live-manifest-policy.md) | Native offline validator: 19 focused checks and all seven required local proofs passed, two independent static security reviews; no durable recovery or installation authority built |
| [superpowers/specs/2026-10-24-live-manifest-native-signature.md](superpowers/specs/2026-10-24-live-manifest-native-signature.md) | Native authentication: nine focused checks and all seven local proofs passed, two independent security reviews; no installation authority |
| [superpowers/specs/2026-10-24-live-manifest-cache.md](superpowers/specs/2026-10-24-live-manifest-cache.md) | Offline persistence: 15 failure scenarios and all seven local proofs passed, two independent security reviews; production initialization/recovery and app integration unbuilt |
| superpowers/specs/2026-10-11-faber-redesign-design.md | Absorbed into product/ and design/ |
| superpowers/specs/2026-10-11-auto-quota-pacing-design.md, plans/2026-10-11-auto-quota-pacing.md, auto-quota-pacing-report.md | Shipped |
| superpowers/plans/2026-10-11-b2 … b9 (test runner, looks, running engine, layout, tray, first run, Settings, download) | Shipped |
| superpowers/specs/2026-10-12-self-development-mvp-design.md, plans/2026-10-12-self-development-mvp.md | Superseded by R202: self-development runs through the project conversation ([dev/self-development.md](dev/self-development.md)); the mission engine was retired with task 060 (R215) |
| superpowers/specs/2026-10-12-faber-rework-design.md | Proposal, nothing built |
| superpowers/specs/2026-10-12-generated-pipelines-design.md, 2026-10-12-tool-selection-rework-design.md | Proposals, partly superseded by the rework design |
| superpowers/specs/2026-10-12-ux-sentinel-design.md, plans/2026-10-13-ux-sentinel-s0.md | Slice S0 shipped (dev/sentinel.md); the rest is a proposal |
| superpowers/specs/2026-10-14-test-system-rework-design.md, plans/2026-10-14-test-system-rework.md | In progress: step 0, R189's waits, parallel scenarios and build skip, and the meter (`faber-bench`) landed; the shared app shelved (R395); the measured report and the real-work tier still to come |
| superpowers/plans/2026-10-22-real-work-bench.md | In progress: step 1 done (how TL-ADE is fed a recorded answer and its quota); the real-work tier of R113, 7 tasks through each app's screen inside the meter |
| superpowers/specs/2026-10-16-browser-preview.md | Proposal, nothing built (recommends yes, after a week of visual critiques) |
| superpowers/plans/2026-10-14-parallel-work-loop.md | The development loop (R128, R129); its rounds table is kept current |
| superpowers/specs/2026-10-16-own-lessons-into-faber.md | Superseded by the own-work-method spec (R435), which says what became of each row |
| superpowers/specs/2026-10-17-built-in-verifier.md | Approved (R190), in progress: V0 measured ([research/2026-10-17-verifier-v0-measurements.md](research/2026-10-17-verifier-v0-measurements.md)), V1 built (the `verify` tool, R203, and its memory cap; its events move to V5), V4 (one shared test app) measured and shelved (decision of 2026-10-21). Faber's own verifier in the engine, checks in seconds on the PC, GitHub only confirms |
| superpowers/specs/2026-10-22-panes-tabs-and-space.md | Proposal, queued as tasks 010 to 060: column views sized by kind, a browser with tabs that remembers, the quick question as a side chat, state Faber keeps, words and keys |
| superpowers/specs/2026-10-17-agent-pane-webview-spike.md | Spike done: go for step 7 of "Watching the agent work", with the five rules in its Recommendation |
| superpowers/specs/2026-10-19-legible-agent-runs-design.md | Approved (Erick, 2026-10-19), tasks 019 and 020 landed (#224, #226). Design: a finished reply looks finished, a done card, the plan, status and closing-box tools, one block per Keep going run, plain words with a technical tab |
| superpowers/specs/2026-10-20-rewind-design.md | Approved (Erick's three answers, 2026-10-20; seventh fresh review), task 090 landed (#191): go back to before a message, files, conversation or both, with redo |
| superpowers/specs/2026-10-21-look-around.md | In progress, steps 1 and 2 landed (R369), step 3 landed (R386), step 4 counting built (R403), the week of use and the decision still to come: after a fix, the same mistake looked for elsewhere and asked about, scoped to the close of a fix and a red-to-green sign, never a rule for every turn |
| superpowers/specs/2026-10-22-own-work-method.md | Approved (R435), slices queued as tasks 991 to 994, nothing built yet: one work method Faber keeps in a project's `.faber/` (rules, decisions, taste, lessons, follow-ups), sized by risk, and the same one for Faber's own repo |
| research/2026-10-21/catalog-list.md | Catalog step 2 curation completed locally; proof results and remaining native validation |
| research/2026-10-22/benchmark-light.md | First run of the light benchmark: Faber against plain Claude Code on 5 requests; Codex rows wait for its quota |
| research/ | Measurements, probes and reviews of the day in their name; model names and prices in them go stale fast. The UX, notification and screen-flake reviews of 2026-10-12 live here; their conclusions are in design/ and product/ |
| superpowers/plans/2026-10-22-frontend-excellence.md | Completed selected changes: original frontend skills, reliability repairs and scoped proofs; initial 243-item audit and unverified criteria retained |

A new spec or plan goes to `superpowers/specs` or `superpowers/plans` with the date in its name and a `Status:` line at the
top (proposal, in progress, shipped, superseded by …), and gets a row here. While it is in progress, each landed step
ticks its box and updates the `Status:` line, so a fresh session resumes from the file instead of from memory. When it
ships, its behaviour moves into product-map.md and the design/ or product/ page it belongs to, and its row says shipped.
Default searches skip `superpowers/` and `research/` (`.ignore`); name the path to search them.

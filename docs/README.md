# Faber's documents

Which page answers which question; dated pages live in history/. When a living page and a dated one disagree, the living
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
| [decisions.md](decisions.md) | To check a change against the decisions in force (R-numbers; the rest are in [history/decisions.md](history/decisions.md)). Search it for the words of your change |
| [product/README.md](product/README.md) | Before changing scope: what Faber is, principles, goals, phases and non-goals |
| [product/models.md](product/models.md) | Auto, model choice, or how the CLIs are driven |
| [product/areas.md](product/areas.md) | Chat and Projects, the toolbox, guardrails |
| [product/lead-agent.md](product/lead-agent.md) | The lead agent, universal by design: aiming higher, normal and autonomous modes |
| [product/building.md](product/building.md) | How Projects builds software: team, workers, self-development, coherence |
| [product/git-workflow.md](product/git-workflow.md) | Portable Git housekeeping, separate-copy reuse, dependency caches and project checks |
| [product/frontend.md](product/frontend.md) | Original frontend design/proof skills, how both AIs receive them, what delivery checks do and do not prove |
| [product/following.md](product/following.md) | Following and watching the agent work, visual answers, learning and memory |
| [product/cli.md](product/cli.md) | The `faber` command line: the local channel, the five reaches, conversation states, verbs, JSON and exit codes, `watch` and `wait` |
| [product/architecture.md](product/architecture.md) | Processes, memory and CPU budgets, updates, the curated catalog |
| [product/recovery.md](product/recovery.md) | Desktop-shell migration, draft import, rollback files and engine restart limits |
| [product/distribution.md](product/distribution.md) | License, provider terms, distribution |
| [product/quota-pacing.md](product/quota-pacing.md) | Auto's quota pacing, quota readings and their measurements |
| [product/parity.md](product/parity.md) | What Claude Code, Codex, claude.ai and ChatGPT have that Faber still lacks, how each gap closes, and what is never closed |
| [design/README.md](design/README.md) | Before any screen change: the design in short, rules for every screen, the first frame, the screen cards |
| [design/looks.md](design/looks.md) | Themes, presets, colour, type, motion, the one grammar, idle cost, effects |
| [design/scenes.md](design/scenes.md) | Static drawings for empty screens and first run |
| [design/brand.md](design/brand.md) | The mark, the app icon, empty states, brand rules |
| [design/icons.md](design/icons.md) | The drawing of each kind of step and each state, and who is speaking: when a screen needs a picture for one |
| [design/homes.md](design/homes.md) | Chat and project homes, including recent conversations |
| [design/answer-line.md](design/answer-line.md) | The line under an answer: its buttons, hover bar, the reply's state and why it stopped |
| [design/conversation.md](design/conversation.md) | An open conversation: the answers and their bar, your messages, the viewer and linked research reports |
| [design/answer-review.md](design/answer-review.md) | Per-answer saved file comparisons, paging, loading and read failures; shared restore history |
| [design/chat-workspace.md](design/chat-workspace.md) | Private Chat files, Windows startup recovery and ownership of selected folders |
| [design/work-log.md](design/work-log.md) | The work log above an answer: its steps, folds and Technical details line |
| [design/reading.md](design/reading.md) | Reading width, selections, durable scroll position and the latest-message control |
| [design/visual-answers.md](design/visual-answers.md) | Native inline pieces, saved choices, generated HTML/SVG cards and their web-access rules |
| [design/layout.md](design/layout.md) | Window widths, titlebar, the homes, the running column |
| [design/chat-groups.md](design/chat-groups.md) | Chat groups, shared instructions and knowledge files |
| [design/sidebar.md](design/sidebar.md) | Sidebar area lists, pinning, export, archive and footer |
| [design/composer.md](design/composer.md) | The shared message box: file mentions, commands, earlier messages and their search |
| [design/branch.md](design/branch.md) | "New conversation from here" and the arrows between the versions of a message |
| [design/queue.md](design/queue.md) | Queued messages, in-place editing, keyboard controls and drafts when a message leaves the queue |
| [design/rewind.md](design/rewind.md) | The message menu's Go back, Redo and recovery confirmation, file preview and saved-message notice |
| [design/visual-answers.md](design/visual-answers.md) | Reply pages, captured file versions, Mermaid, isolation and measured bounds |
| [design/running.md](design/running.md) | The running column: rows, question cards, done cards, when it opens by itself |
| [design/keep-going.md](design/keep-going.md) | Keep going: the chip, the quiet lines, the quota warning, the running row and the summary on coming back |
| [design/search.md](design/search.md) | Sidebar message search, the command box (Ctrl+K) and find in a conversation (Ctrl+F) |
| [design/subagents.md](design/subagents.md) | A reply's subagent and helper rows, their running-column view, the stuck signs and the pages opened in Faber's browser |
| [design/catalog.md](design/catalog.md) | Catalog browsing, installation, on/off, Undo and per-source inventory |
| [design/settings.md](design/settings.md) | The Settings screen |
| [design/permissions.md](design/permissions.md) | Permission choices and safeguards |
| [design/first-run.md](design/first-run.md) | The first run and the tutorial |
| [design/projects.md](design/projects.md) | The Projects screens, what a project stores, and how its checks run |
| [dev/testing.md](dev/testing.md) | Tests: `cargo t`, screen tests, build folder and slots, what a run prints |
| [dev/evals.md](dev/evals.md) | Opt-in evaluations of what the AI writes with the real CLIs: the router, the persona |
| [dev/cli-qualification.md](dev/cli-qualification.md) | Nightly unauthenticated CLI candidate contracts, bounded metadata receipts and missing compatibility evidence |
| [dev/plan-boundary-qualification.md](dev/plan-boundary-qualification.md) | Task 500 approval-layer size admission, recorded coverage and ungated observations |
| [dev/antigravity-cli.md](dev/antigravity-cli.md) | How the Antigravity CLI (Gemini) behaves, recorded: output events, approval, resume, isolation, MCP, quota |
| [dev/build-folders.md](dev/build-folders.md) | Build-cache cleanup, disk budget and preserving exact proof artifacts |
| [dev/fair-bench.md](dev/fair-bench.md) | The fair benchmark against TL-ADE: the meter, the suite and real-work tiers, the notes |
| [dev/release-1.0.md](dev/release-1.0.md) | Before cutting 1.0: the release checklist |
| [dev/benchmark.md](dev/benchmark.md) | Running the light benchmark: the one command, what a run measures, what it leaves out |
| [dev/workflow.md](dev/workflow.md) | The lanes (how many, how big, how reviewed), lane claims, the task queue, a red base and the numbers given at landing |
| [dev/landing.md](dev/landing.md) | Merge on green: the queue, batches and local checks before landing |
| [dev/whats-new.md](dev/whats-new.md) | Writing a what's-new entry: the words, the kinds, numbers for speed and memory |
| [dev/sentinel.md](dev/sentinel.md) | The UX sentinel, the visual critique and the first-user review |
| [dev/dev-edition.md](dev/dev-edition.md) | Faber Dev: the owner's separate copy that follows the green base and opens as a new person every time |
| [dev/self-development.md](dev/self-development.md) | Faber building Faber from its own project conversation: how to start it and which part does what |
| [security-r97-review.md](security-r97-review.md) | The verified security review; its R121/R124 section is kept current |
| [tasks/README.md](tasks/README.md) | What a session does next, in order, one file per task (`node scripts/tasks.mjs next`) |
| [Antigravity driver](tasks/later/010-antigravity-driver.md), [Antigravity in the app](tasks/later/020-antigravity-in-the-app.md), [several models at once](tasks/later/060-several-models-at-once.md), [switch vendor mid-conversation](tasks/later/070-switch-vendor-mid-conversation.md) | The T3 Code study tasks, waiting until after 1.0; its needs-you-first list, readable turns and palette ideas went to the rework of the thread, endings and command layer |
| [brand/](brand/) | Logo sources |

## In progress and history

| Page | Status |
| :--- | :--- |
| [superpowers/plans/2026-11-05-lean-faber.md](superpowers/plans/2026-11-05-lean-faber.md) | In progress: the lean plan. D1 fast lane (R556), D2 paperwork (R557), D3 documentation diet and D4 fewer lanes landed; the rest is proposed |

[history/](history/): history, never a source for how Faber works today. Dated specs, plans and research (under
`history/superpowers/` and `history/research/`) and the decisions no longer in force ([history/decisions.md](history/decisions.md)).
A search skips it (`.ignore`); name the path to read it. A new spec or plan goes to `superpowers/specs` or
`superpowers/plans` with the date in its name and a `Status:` line at the top; once it ships or is dropped it moves to
`history/` with `git mv`, its behaviour already in product-map.md and the design/ or product/ page it belongs to.

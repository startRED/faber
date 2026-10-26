# Faber roadmap

What Faber is, its principles and goals, and the order of work. Each part of the plan has its own page in this folder;
[docs/README.md](../README.md) says which page answers what.

Status: Phase 1 (the new base) is in progress. The base app already works: the engine, the Tauri shell, Chat, Projects,
Auto, five languages and the updater. What these pages describe beyond that base is still to be built, in the order of
[Phases](#phases).

## What Faber is

Faber is a desktop app that runs the official Claude Code and Codex CLIs on the user's own subscriptions.

- It aims to beat every harness and meta-harness we know, in orchestration, quality, optimization and token economy.
- It does anything AI can do, at commercial quality, so the user never needs another AI app. It has everything Claude
  Code and Codex have.
- It is for anyone. People who use AI every day should find it familiar, and someone who never has should learn it just
  by using it. "Enterprise-grade" means professional quality and robustness, with maintenance and updates that happen on
  their own. Team and admin features are out until they are asked for.

## Principles

- **Priorities.** Quality is roughly equal to economy, and both come before speed. Quality is slightly ahead. When speed
  and economy conflict, economy wins.
- **Good at everything from day one.** The order of the phases decides which areas get specialized tools and benchmarks
  first. It never limits what Faber does. The floor is the unmodified CLI underneath, with nothing cut.
- **Only Claude Code and Codex.** Sign-in always happens in the CLI's own flow. Faber never reads, copies or stores
  credentials.
- **Every decision has an author.** The decision ledger records each decision as the owner's or the AI's. The owner's
  decisions outrank the AI's proposals. The AI asks the owner only about product direction, taste in visible identity,
  money, license, and anything that contradicts an earlier decision of the owner.
- **Light.** Memory is a feature, with a hard budget (see Architecture).
- **Nothing third-party ships with the app.** Telemetry is off by default.
- **Faber builds itself.** Claude and Codex build Faber together, and whoever writes a part never reviews it. Faber's
  own project conversation does it, and builds the rest of Full Chat (R96;
  [self-development](../dev/self-development.md)).

## Goals

**The bar:** working through Faber must beat talking to a model directly (in Claude Code, Codex, claude.ai or
ChatGPT) at everything. Nothing a direct session can do may be missing or worse in Faber; the goals below measure
where Faber must be better.

Before each version, the benchmark measures it against plain Claude Code and plain Codex on the same ~40 real requests
(organizing a folder, planning an app in conversation, a complete app, a thumbnail, a video, a 3D model, a game mod,
research). The version ships only with the maintainer's OK, and a version that makes any goal worse does not ship.
Creative results are judged blind, shown as "A" and "B", and the answer is revealed after the vote.

| Goal | Measure | Target |
| :--- | :--- | :--- |
| Quality (Projects) | blind judge from another vendor plus executable proof | wins or ties 9 of 10 requests |
| Quality (Chat) | blind judge against the same question asked on claude.ai and ChatGPT; sources checked | wins or ties 8 of 10; zero invented citations |
| Quota and tokens | tokens per approved task, priced at API rates (input, output, cache) | ≤ 70% of the plain CLI's cost for the same task |
| Speed | time to first token with a warm session | ≤ 2 s |
| Precision | corrections the user asks for after "done" | half of the plain CLI |
| Efficiency | idle RAM without a CLI open; installer size; sessions broken by CLI updates | under 500 MB (the test fails above 400 MB, R125); ≤ 120 MB; 0 |

### Measuring without burning the user's quota

- Cost is measured in tokens reported by each CLI, priced at API rates, never as a share of the subscription.
- Each version runs a rotating sample of ~8 requests; the full ~40 run only for major versions, starting with the first
  public one.
- Plain-CLI baselines are recorded once per model version and reused.
- The judge is blind: answers arrive anonymized in random order. A cheap model screens; a strong model only breaks
  close calls.
- Each run has a budget in API-equivalent dollars (initially about US$ 3 for the sample and US$ 20 for the full set,
  adjustable in settings). Every model is priced at its own rate, per vendor, model and effort, from a dated price
  table (models.dev and LiteLLM), and the run stops when the budget is reached.
- Runs wait while someone else is using the account (see shared-account mode below).
- Subscriptions limit usage in different shapes (rolling windows, weekly caps, monthly caps, or none). Each CLI
  declares its own shape in its driver descriptor; nothing in Faber assumes a 5-hour window.

### Shared-account mode

Many people share one subscription (family, small teams). Faber watches the quota readout: if it drops faster than
Faber's own usage explains, someone else is using the account, so background work (benchmarks, harvesting expiring
quota, re-runs) pauses until the account is quiet again.

## Phases

The order of the phases says what gets specialized tools and tests first. Faber does everything from the start. Before
each version, the benchmark measures it, and the version ships only with the maintainer's OK.

| Phase | Scope | What comes with it |
| :-- | :--- | :--- |
| 1 | New base | the FSL-1.1-ALv2 license and these documents, Faber's own Rust test runner, the design system and themes, the identity, the shell layout and running list, the tray number and notifications, the first run and tutorial, Settings, and the CLI download written straight to disk. Every user-facing change already ships as a patch release (R130) |
| 1a (R96) | Self-development | the project conversation that builds Faber ([self-development](../dev/self-development.md)): separate copies, conversations the agent starts, Keep going, following its pull request, the built-in verifier and Faber's own checks; the missions plan it replaced is history |
| 2 | Full Chat | the benchmark, visual answers, asking Faber about Faber (built, a file of Faber's own every session can read), search, moving a chat to a project, Auto learning from approvals (built) and reporting failed writes of conversation and project files (built) |
| 3 | Projects with the lead agent | the lead agent, toolbox, 15 powers, modes, guardrails and big-work rules, the rules for running many workers, the progress view, watching the agent work (living sentences, side chat, Faber's browser and app pane), the decision list, the catalog, learning and the `.faber/` folder, and rules 4 and 7 | |
| 4 | Excellent software | generalize the self-development workflow; risk-sized independent plan review and delivery-to-plan checks, starting with a measured Faber pilot ([implementation proposal](../superpowers/specs/2026-10-26-independent-plan-review.md)); nightly CLI version tests and the live manifest, shared project MCP infrastructure, and qualified department writers |
| 5 | First public version | |
| 6 | Image and design | creative drafts, blind judging and comparison views |
| 7 | Phone and routines | the phone as a remote control, messaging channels and routines |
| 8 | Video | |
| 9 | PC apps | |
| 10 | Mods and reverse engineering | |

Not planned yet: community themes listed in the catalog, code signing and the Microsoft Store, and animations for the
symbol.

### Proposed work-method extension

The [Faber Flow revision 3 proposal](../superpowers/specs/2026-10-22-faber-framework/README.md) researches company and
community practices and extends the approved R435 method without replacing its records or changing the phase order.
Its [capability backlog](../superpowers/specs/2026-10-22-faber-framework/features.md) and
[implementation slices](../superpowers/specs/2026-10-22-faber-framework/roadmap.md) cover current-input identity,
complete task packets, evidence-linked outcomes, adaptive support for light/frontier models, constrained composition,
recovery, calibrated review, domain checks, readable progress and measured workflow improvement. Reusable work becoming
scheduled routines remains in phase 7. These are proposed capabilities, not approved implementation or measured gains;
existing R435 work retains precedence and each candidate needs normal adoption and program evidence.

### Non-goals

- Vendors other than Claude Code and Codex: no Gemini, Copilot, Cursor, OpenCode or ACP drivers.
- A path with no subscription, such as local models or Faber holding an API key.
- Team, admin and permission features.
- Driving a chat website or copying its login session, and using ChatGPT's connectors.
- Shipping any third-party tool, skill, model or MCP server inside the app or the repo.
- Telemetry that is on by default.
- A phone app that runs work. The phone is only a remote control.
- A separate help page. People ask Faber about Faber.

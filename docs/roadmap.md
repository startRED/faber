# Faber roadmap

Status: Phase 1 (the new base) is in progress. The base app already works: the engine, the Tauri shell, Chat, Projects,
Auto, five languages and the updater. What the sections below describe beyond that base is still to be built, in the
order of the Phases section.

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
- **Faber builds itself.** Claude and Codex build Faber together, and whoever writes a part never reviews it. Once the
  self-development MVP is ready, Faber builds the rest of Full Chat itself (R96).

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
| Efficiency | idle RAM without a CLI open; installer size; sessions broken by CLI updates | under 500 MB (the test fails above 250 MB today); ≤ 120 MB; 0 |

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

## Only Claude Code and Codex

Faber works with Claude only, Codex only, or both. It supports no other vendor.

- **The subscription is the main path.** An API key is optional and goes only through the CLI's own login, so Faber
  never sees it. Nobody needs an API.
- **First run.** The user connects what they have. Faber finds or installs each official CLI and opens its own sign-in,
  and the router and the toolbox only use what is connected. A person who is new to Claude, Codex and AI apps gets a
  short tutorial; an experienced one skips it. After that Faber does not keep teaching: there is no help page, and
  people ask Faber about Faber.
- **Both vendors:** reviews and second opinions come from the other vendor.
- **One vendor:** reviews come from a different model of the same vendor in a fresh session, and the screen says the
  review is weaker.

### Auto: a blind router

Each vendor's model tends to prefer its own company's models, so in Auto no model ever sees a model's name or a vendor's
name. Auto aims to pick a model that does the task well, without waste and quickly.

- A small, fast model that is already running (GPT-6 Luna or Haiku at medium effort, whichever is connected) reads the
  request alone and says what kind of task it is and how hard. It never sees the candidates.
- A fixed table then picks the model, tuned for quality first: each model's profile has general intelligence and
  agentic coding scores, seconds per task, and cost per task priced by the quota left on its subscription right now, so
  a nearly spent subscription hands its work to the other one (done, `engine/src/route.rs`).
- Measured on 10/10/2026 with the real router on 22 requests, run twice: 81 of 88 picks right, half the routes under
  2.8 s and nine in ten under 3.9 s. An earlier design that let the router choose among anonymous, shuffled profiles
  got 76 of 88, spent more and was slower: the router judges requests well, the table weighs price against quality.
- Profiles start from public benchmarks (Artificial Analysis, read 22/09 to 08/10/2026, compiled in for now) and move
  to Faber's own scoreboard as it fills: which answers users kept, redid or rejected. Later they ship in the live
  manifest, so a new model joins without an app release.
- No local model and no hosted classifier: routing spends only the subscriptions the user already has.
- Routing runs on the first message of a conversation, and on the next ones while only an easy model was needed (a
  greeting first). Those stay with their subscription, so the CLI keeps the history, and only move up. Once a message
  needed more, the conversation stays with that model. A sent message waits at most 30 s for its route; past it, the
  turn starts with the everyday default. The answer shows the chosen model and the reason.
- **Still to build.** The table learns from what the user approves or redoes. The draft is routed at most twice per
  message while the user types; today it is routed at every pause.
- The router is measured too: on the benchmark of real requests, its picks must match always using the strongest
  model while spending less quota.

## Two areas

- **Chat** works like claude.ai and ChatGPT, and must beat them. It has many conversations and no folders, and it never
  changes files on the computer: it only reads what the user attaches. Each conversation has a hidden workspace inside
  the app where the model runs code, draws charts and creates files. The user never sees a folder. Files show up in the
  conversation with "download" and "take to a project", and they stay in the workspace until the user saves them or
  moves the conversation to a project. Commands that could reach outside it ask first. The user picks any connected
  model or Auto, and every answer shows the model and, in Auto, the reason in one line.
- **Projects** works like Claude Code: open a folder, keep many projects and many conversations per project. This is
  where the lead agent acts. A Chat conversation can move to a project with its full context.

The user always sees what is happening: the model behind each answer, the remaining quota of each subscription and
when it renews, and, for running work, who is building and who is reviewing.

## Universal by design

There is no fixed pipeline per kind of request and no list of supported tasks. In Projects, the lead agent is a live
session of one CLI. It gets a toolbox from Faber (an MCP server shared by every CLI) and picks the path for each
request itself. Before big work it shows the plan, the time and the quota, and asks once. Then it decides each step:

- solve it right away, with undo ("organize my downloads folder");
- talk it through and plan ("I want to build an app");
- start a mission with several models in parallel ("build the whole app, ready to ship");
- find what it lacks: search the catalog and beyond, show the license, install from the source, test on a copy and
  save the path as a recipe ("make a Terraria mod").

**Powers, not tracks.** A fixed line per kind of request (thumbnail, video, site, research) runs steps a request does
not need and has nothing for a request no line covers. Faber gives the lead agent the powers those lines rely on and
lets it compose them per request, the way a senior professional would. The agent has 15 powers, and the toolbox below
is how they work:

| Power | What it gives the agent |
| :--- | :--- |
| Ask only what changes the result | a question card with a default that applies by itself if nobody answers |
| Learn from what worked | find the outliers in a topic (videos, titles, sites) and why they worked, with links |
| Cheap draft before expensive work | rough sketches or three quick versions, picked before hours go into the final one |
| See it where it will live | render the result at its real size and context (a thumbnail in a phone feed, a page on a phone) |
| Another model checks | a blind review or judge from another vendor; in code the writer never approves its own work |
| Better than the original or it does not stay | blind side-by-side of the edit against the original; the original wins ties |
| Proof before work | the agent states how success is checked, then runs it; "done" needs that evidence |
| Facts with sources | every claim tied to the page that proves it; numbers computed by a program and recomputed |
| Redo only the part that changed | deliverables are pieces with owners and dependencies; a change reruns the owner and what depends on it |
| Pick up where it stopped | every step is durable; killing the engine or rebooting loses nothing |
| Drive the user's apps | Blender, Photoshop, Premiere and others through their scripting, on a copy |
| Remember taste | the user's opinions on past work steer the next one; questions shrink with use |
| Right-sized | each part of the work goes to the model, effort and context that are cheapest and still do it well; it moves up to a stronger one only if the check fails |
| Don't reinvent | before creating from scratch, look for ready work with a good license, including the user's own past work, and use or adapt it |
| Safe to sell | before delivery, check for leaked keys and passwords, known security flaws, and whether the licenses of everything used allow commercial use |

Recipes from past work are hints the agent may read, never a track it must follow. The guardrails below are the only
fixed rules.

### Normal and autonomous modes

In both modes the agent decides what it can and asks only what changes the result; it never stops just to report or to
wait. Everything persists across closing, crashes and reboots.

- **Normal**: the user works alongside. Faber feels like Claude Code: the user talks only to Faber, and Claude and Codex
  work behind it. A question card waits for the user, with its default ready, and the actions guardrail 3 covers ask for
  approval.
- **Autonomous**: nobody needs to be there. Question cards apply their default at once and log it. Sending,
  publishing, paying or deleting outside a copy is never approved on its own: it is parked with a note and the rest
  goes on. Changes to Faber itself follow the user's trust setting, parked without full trust. The work runs to the end,
  then leaves a report: what was done, what was decided alone and why, what is parked.
- **Switches at any moment, effective at once**: one click in the conversation (and a shortcut), also while the agent
  is answering or a mission is running. Turning it on resolves open question cards with their defaults and parks
  waiting approvals, without restarting anything: the user who was working in Normal and has to sleep just turns it
  on. Turning it off brings the next questions back to the user.

### Building software: one voice, a team behind it

Normal mode feels like Claude Code: the user talks only to Faber. Behind it, Claude and Codex sessions work in
parallel, each on what it does best, and the result must read as if one careful team made it.

- **The lead keeps the thread.** One live session plans, splits the work and answers the user. Workers get a short
  brief and return a short report; a worker that stays on a part keeps its session while the prompt cache is warm, and
  each step loads only what it needs.
- **One map of the product, read before and updated after every change.** The design system (tokens, components,
  type, spacing, voice) and an inventory of every screen, button and flow with its purpose. A worker building UI starts
  from it; nothing ships that the map does not account for.
- **No duplicates, no strangers.** Before a new screen, button or setting, the agent searches the inventory for one
  with the same purpose and extends it instead. Code checks enforce the design system: only its tokens and components.
  A coherence pass looks at the whole app, every screen side by side at each size, not just the diff, and flags
  anything that does not look or behave like the rest.
- **Design starts from references.** Three directions grounded in real references come first; in autonomous mode the
  two judges from different vendors pick, with a written rubric. The final screens are checked in a real browser at
  1920, 1440 and phone width.
- **Done means it works.** The proof is written before the work and run after it: unit tests, end-to-end runs in a
  real browser or the real app, and a ghost user from another vendor that uses the result like a person, recorded.
  A bug hunter tries edge cases and broken paths on purpose. Every bug found gets its root cause fixed and a test that
  fails without the fix.
- **Everything persists.** The mode, the plan, each worker's progress and session, the product map, open question
  cards, parked actions and the report are saved as they change. Closing Faber, a crash or a reboot resumes exactly
  where it was, autonomous runs included; Faber keeps working from the tray when its window is closed.
- **Measured against the TL-ADE.** The same requests run on both: design scores from blind judges, bugs found after
  delivery, coherence findings, time, tokens and how often the user had to step in.

**Big work always reaches a conclusion.**

- "Stop" pauses, and the work resumes from where it stopped.
- Going 50% over the estimate only warns and switches to economy mode: cheaper models for support work, extras put
  off. Faber preserves unfinished work. Projects narrows or pauses before its live worker, mission and daily cost caps;
  an estimate overrun only warns. R88/R90/R91 govern admission, not the estimate.
- A stuck attempt is dropped while what it produced stays. The lead tries another path (another approach, a stronger
  model or one from the other vendor, a clean session) and only then asks the user about that one point, while the rest
  goes on.
- When a quota runs out, the work moves to the other subscription, or waits and resumes by itself at the renewal.

**Decision list.** Every project keeps a decision list with authors. The AI's decisions show as "not reviewed".

### Toolbox

| Tool | What it does |
| :--- | :--- |
| `mission.start` / `mission.amend` | runs a plan written by the agent as a durable graph of steps across several CLIs |
| `worker.spawn(role, vendor)` | opens a worker in another CLI to research, write or review |
| `review.request` | Faber picks a reviewer from another vendor, in a fresh session |
| `proof.define` / `proof.run` | states up front how success will be checked, then runs it and keeps the evidence |
| `judge.creative` | two judges from different vendors, in fresh sessions |
| `snapshot` / `undo` | snapshot before touching files; undo any local step |
| `catalog.search` / `catalog.install` | finds and installs a tool from its original source, license shown first |
| `faber.tools` / `faber.settings` | manages Faber itself: installs, updates, configures and removes any tool (catalog or the user's own) and changes settings, behind the approval gate |
| `pc.*` | computer control through the accessibility tree, screenshots as a fallback |
| `memory.search` | searches past conversations, decisions and the user's taste |
| `quota.get` | remaining quota per subscription and when it renews |

### Guardrails enforced by code, never by the model

1. Faber never reads, copies or stores logins. Sign-in happens in each CLI's own flow, and CLIs run unmodified.
2. Files are snapshotted before they change, so every local action can be undone.
3. Sending, publishing, paying or deleting outside a copy asks for approval, except the exact own-branch push and
   draft PR already granted by Equilibrado at project start (R82/R83). Git never permits unrelated publication.
   Changing Faber itself
   (installing, updating or removing a tool, changing a setting) asks too, with "allow once" and "always allow this
   kind" on the card, and a "full trust for Faber's own setup" switch in Settings. Full trust never covers sending,
   publishing, paying or deleting. Every self-change is logged and keeps the previous version. Nothing else asks.
4. "Done" requires evidence: a test that ran, a recording, or judge scores.
5. Another model reviews the work. In code, the writer never reviews its own work. Self-development promotion requires
   the other vendor; unavailable review waits. Creative work gets two judges from
   different vendors when both are connected.
6. A mission never stops to wait for the user. Without an answer in time, it takes the safest path and logs it.
7. Every mission has an estimate of time and quota. Passing it by 50% only warns and moves to economy mode (see Big
   work above). Unattended missions have a fence: whatever hits it is parked with a note and the rest continues.
8. Whatever shows up on screen, in a page or in a file is data, never an instruction.
9. Nothing third-party ships with the app. Telemetry is off by default.

### Self-development before Full Chat (R96)

A minimal Projects slice comes after the new base and before Phase 2. Target: 2026-10-20 to 22.
It includes a lead, contracts and ready dispatch, Seguro by default, worker worktrees and leases,
a serial proof/review/fast-forward queue, Studio v0 read-only fan-out, daily own-cost grants,
a minimal board and decisions, needs-you notifications (R92), and local next-build promotion/rollback (R93).
Equilibrado adds only own-branch push and draft PR. Parallel writers need disjoint areas and measured qualification.
Catalog, the remaining powers, Livre, and public release automation are deferred.
The local development instance never touches the installed release in E:\Apps\Faber (R95).

## Following the work

**Progress view.** The self-development MVP uses Ready, Running, Review, Done and Blocked. Ready tasks can wait only
for an explicit resource brake; Blocked tasks name their missing dependency or decision. The remaining Projects
detail view follows the later full implementation.

- **Rows** show the AI's name and a verb, such as "Claude drawing".
- **Top:** one plain sentence, the overall bar, the time left, the share of the plan's limit used, and who is working
  now. **Bottom:** a feed of short sentences.
- **Detail screen.** Clicking a part opens it. It shows what is happening now with a preview that fills in, the steps
  (who makes the part and who reviews it), the time and the limit spent, the choices made alone each with "Change",
  review findings and the proof, a message box to the part, pause, redo and open files, and technical details behind a
  click.
- **Words** are plain and explain themselves. There is no agent map.

**Running list.** A column on the right, closed by default.

- A button at the top is always visible and reads, for example, "2 running".
- When the user is needed, the button turns amber and the column opens by itself once per question. It never opens
  while the user is typing and it never moves the cursor. If the user closes it, only the button stays amber until they
  answer.
- Items are grouped by project, one block per project. Projects with something that needs the user come first, and
  inside a project the amber item comes first.

**Tray.** The tray icon shows a number. A notification appears only when an agent needs the user, never each time
something finishes.

- **Attention notifications (R92, owner).** One per question, approval or blocked request, and none while Faber is
  focused. Toasts and their original short chime default on, with independent switches in Settings: turning off
  "Notify me" leaves the sound choice alone.
- On Windows, `SHQueryUserNotificationState` selects sound only for a busy full-screen app, an exclusive Direct3D
  game, presentation mode or a Store app. Only the accepts-notifications state permits a toast, and a foreground
  window covering its whole monitor suppresses that toast too (including borderless games, excluding Faber's windows).
  Locked/screensaver and initial-login/upgrade quiet-time states stay silent; quiet time is not Focus Assist/DND.
  Failed or unknown state queries also suppress the toast. Delivery never opens, flashes or focuses a window.
- macOS and Linux send a toast and the chime; full-screen detection comes later. Sound uses Windows' `PlaySound`,
  macOS' `afplay`, or Linux's `paplay` with `aplay` as a fallback. These players come from the system, never the bundle.
- The [notification review](notifications-review.md) records the verified findings and the detection limits.

## Visual answers

Answers can contain visual pieces, such as checklists, charts and forms. Faber calls them "Visual answers", in both Chat
and Projects.

- They start with about 10 built-in pieces: big numbers, table, chart, slider or −/+ tied to a calculation, checklist,
  tabs, steps or quiz, options form, notice and timeline. Free-form mini apps come later, and only if they are missed.
- What the user checks or moves stays saved, even after the conversation closes.
- Text is the default; a piece appears only when it saves the reader effort.
- A form never sends on its own. It only fills the composer.
- A broken piece falls back to plain text.
- Side-by-side comparison of options is part of visual answers: images, animations or live sites, whatever fits.

## Learning and memory

Faber learns from use, in a way that stays visible and organized.

- What belongs to a project stays in that project. Global memory keeps only what holds 100% in every situation.
- A clear objective improvement becomes global by itself after it repeats in 3 or more projects, and it can be deleted.
  A personal preference becomes global only with the user's approval. When in doubt, Faber treats a lesson as a
  preference and asks.
- Everything sits in a visible list with one-click delete.
- Decisions, taste, plans and recipes live in plain files in the project's hidden `.faber/` folder, so other programs
  and AIs can read them. The conversation history of both areas stays private inside Faber.

## Rules for driving the CLIs

These are product rules. They come from research on driving Claude Code and Codex and from the model-use notes. Each is
re-measured when a CLI version changes. Every piece of code that starts a CLI or talks to one follows them, in every
phase.

1. **Never use Codex app-server login.** Faber never calls `account/login/*` and never stores `auth.json`. Sign-in
   happens only through `codex login` or the CLI's own flow.
2. **Always pass the permission mode, the full model id and the effort when starting a CLI session.** An omitted flag is
   a hidden choice.
3. **Set the model and the effort before a conversation starts, and never change model, effort, tools or system prompt
   in the middle of one without need.** The prompt cache is the biggest saving. A change of model always starts a new
   cache, and a change of tools or system prompt invalidates everything after it. A change of effort may keep the cache
   today, but Faber does not count on it. Facts that change travel as text inside the next message, never as a
   system-prompt edit.
4. **Hand work to another model through a written summary or a diff, not the conversation.** Thinking blocks are tied to
   the model that wrote them.
5. **Interrupt gracefully before killing, watch for silence, and kill the whole process tree on Windows.** The stop uses
   the CLI's control-protocol interrupt, waits for the result, then kills the tree.
6. **Read the CLI's start-up capabilities, the result subtype and the terminal reason, and map them to `reason.*`
   keys.** Faber detects features from the `capabilities` list in `system/init`, never by comparing version strings, and never
   shows a raw reason string.
7. **Keep Faber's own running budget, and apply the big-work rules when it is passed.** The CLI's budget flag is only a
   client-side estimate, so Faber counts spend per turn itself. Passing the estimate only warns and changes to economy
   mode.
8. **No SDK prewarm.** A spare process waiting for a folder costs about 230 to 260 MB, which breaks the idle budget.
9. **Keep each Haiku request under 100K tokens and each GPT-6 request under 272K.** Above those points the whole request
   is billed more.

Rules 1, 8 and 9 hold today. The engine that already drives the CLIs has known legacy exceptions to the others. They are
known defects of the old code, no new code may add to them, and each phase closes some:

| Rule | What the engine does today | Closed in |
| :--- | :--- | :--- |
| 2 | Chat sessions pass `--permission-mode`. Projects sessions pass none. A session may start on the account's default model or effort | Phase 2 |
| 3 | Auto's move up changes the model inside a conversation | Phase 2 (check the hand-over) |
| 4 | There is no lead agent yet, so there is no hand-over | Phase 3 |
| 5 | Interrupt sends the CLI's control-protocol interrupt, but nothing waits for the result, nothing watches for silence, and `proc::stop` kills the child alone, not its tree | Phase 2 |
| 6 | `terminal_reason` or `subtype` is passed on as a raw string, and `capabilities` from `system/init` is not read | Phase 2 |
| 7 | The engine keeps no running budget | Phase 3 |

## Architecture

```text
App (Tauri 2: window, tray, notifications, auto-update)
 └─ UI (SolidJS + TypeScript, in the system webview) ── fixed command list ──┐
Engine (Rust crate inside the app today; a separate process later, so work survives the app)
 ├─ Lead agent: live CLI session + Faber toolbox (MCP)
 ├─ Drivers: Claude Code (stream-json), Codex (app-server)
 ├─ Capability manifest per CLI and version
 ├─ Missions: step graph, workers, gates, resume after crash or reboot
 ├─ Router: quality floor + quota price + measured scoreboard
 ├─ Project memory: plain files in the project's .faber/ folder
 ├─ Snapshots and undo · policy and audit log
 ├─ Curated catalog and store
 └─ Live manifest: compatibility matrix, provider policy switch, driver descriptors
```

- **Rust, Tauri 2 and SolidJS.** Chosen on 2026-10-10, while the app was still small, for weight and memory: Tauri
  uses the system webview (WebView2 on Windows) instead of shipping Chromium, and the engine is native code with no
  Node runtime. SolidJS updates only what changed, which suits answers streaming in. C++ only where a library leaves
  no choice. The cost: three webviews instead of one Chromium (WebView2, WebKit on macOS, WebKitGTK on Linux), so
  anything heavy on screen (terminal, 3D viewer, video) is tested on all three. The first Electron version stays
  tagged as `electron-f1`.
- **A memory budget, enforced by a test.** Faber itself (window open, no CLI running) stays under 500 MB when it is
  complete; the end-to-end test fails above 250 MB so there is room for what is still to come. Anything that keeps a
  process or a large DOM alive needs a reason and a measurement. Measured on 2026-10-10 as Task Manager counts it
  (private working set): 89 MB idle, 114 MB with a thousand-message conversation open (only the last 40 messages are
  drawn, more on scrolling up), 4 MB closed to the tray (the screen is freed; the engine keeps running and the window
  comes back where it was). For comparison, the Claude desktop app used 2.5 GB that day.
- **CLIs that cost almost nothing while idle.** A Claude Code process takes about 120 MB just waiting (more in long
  sessions, and each session starts its own copies of the user's MCP servers), and it has no way to serve two
  conversations. So Faber starts it on the first keystroke, not when the window opens, and closes it after two idle
  minutes; the next turn resumes the same session. Codex's app-server can serve many conversations as threads, so one
  shared process per area holds them all (about 50 MB for three). Next: one shared copy of each MCP server for all
  sessions of a project, and a RAM budget across all CLIs once the lead agent runs workers in parallel (workers queue
  instead of going over it).
- **Languages.** English, Portuguese (Brazil), Spanish, French and German from the start, following the system
  language unless the user picks one in Settings. Every text is written by hand, never machine-translated; a new
  language needs a native speaker to review it.
- **The user's own setup comes along.** Projects use the skills, MCP servers and plugins already configured in each
  CLI, and Faber's catalog installs into that same configuration, so a tool works the same inside and outside Faber.
  The user's hooks are off by default (they can rewrite answers or block steps) and can be turned on in Settings.
- **Use everything each CLI has.** Each driver declares, per CLI version, what exists: resume, fork, MCP, skills,
  plugins, hooks, subagents, plan mode, sandbox, web search, image generation, computer use, review, quota readout.
  A probe checks this on install and on every update, reading what the CLI reports about itself, so new CLI features
  show up by themselves. If Faber cannot parse a CLI's output, it falls back to the raw terminal in one click instead of
  freezing.
- **Project memory is plain files.** A project's decisions, taste, plans and recipes sit in its `.faber/` folder, in
  open formats, with a README that explains them to any future AI. Each deliverable keeps a re-runnable recipe and
  carries its request and taste as provenance. The conversation history stays private inside Faber.

## Updates

Local self-development uses current/next slots and a retained last good. Exact-tree proofs and a small canary gate
promotion. Swap only with no window and no active effect. An independent helper rolls back an early failure.
Saved data stays compatible with last good. This channel is separate from signed public updates (R93–R95;
[spec](superpowers/specs/2026-10-12-self-development-mvp-design.md)).

- **Faber itself** updates itself. Today it downloads a signed update in the background and installs it on quit, so it
  never interrupts work.
- **The CLIs.** Claude Code and Codex move only to versions that GitHub tested overnight with recorded conversations.
  If a version breaks, Faber stays on the previous one until a fix arrives as a published file, with no new Faber
  download.
- **Catalog tools** never update on their own. Faber suggests an update at a good moment, which is when nothing is
  running or when the update fixes something the user is using, and never in the middle of work. If the user agrees,
  Faber runs the update and the check. Before updating, it shows what changed, checks safety and keeps the old version.

## What makes it better

- **Measured quality.** The proof is defined before the work. A ghost client from another vendor uses the result like
  a regular person and records the screen. Reviewers are checked with a planted defect on a copy, adopted only after
  offline validation. When a new model ships, Faber replays the user's best past work and reports what improved.
- **Quota as a perishable resource.** For providers whose limits reset, Faber spends what would be lost near the
  reset on deferrable work. One to three CLIs stay warm within a visible RAM budget. The CLI starts booting at the
  first keystroke. Each step loads only the tools it needs, and related requests stay in the same session while the
  prompt cache is warm.
- **Safety for non-programmers.** Work in external apps happens on a copy the user promotes with one click. Canary
  tokens catch malicious catalog items. A stateful policy blocks any command it does not understand.
- **Learned taste.** Early on, Faber shows more options and learns preferences; later the questions shrink.

## Curated catalog and live manifest

The catalog screens and the catalog step in the first run arrive in Phase 3, with the lead agent's toolbox.

- **Faber recommends only tools it has vetted.** A vetted tool works, is safe, has a license that allows the use, is
  maintained and improves the result. The agent may find and install any other tool. That tool is marked "not vetted",
  and the agent asks before installing it.
- Nothing ships in the installer. On first run the user picks the areas they work in; Faber shows the packs for
  those areas with the recommended items already ticked. Everything is optional: untick anything, or press "Install
  all recommended". Installs run in the background while the user starts working. Each item downloads from its
  original source with a pinned version, a checksum and its SPDX license on screen. Non-commercial licenses get a
  warning. Models that require accepting a license on their hosting site ask the user to accept it there.
- **Curation is strict.** An item enters only if it: has a license that allows the use; is maintained; installs
  from source in an automated smoke test; passes the security scan; is not a weaker duplicate of another item; and
  works with at least one supported CLI. Each item has a tier (Essential, Recommended, Extra, Experimental), the areas
  it serves, the CLIs it works with and its requirements (OS, GPU memory, apps such as Blender). "Recommended" needs
  evidence: the benchmark shows it helps, or it is the reference tool for its job. The first draft of the list has 1
  Recommended item, 123 Experimental items and 28 removed, and it keeps growing and getting curated over time.
- **Video and music are covered.** Composed video (HyperFrames plus FFmpeg) works with any subscription, because the
  CLI writes the composition. AI-generated music, voice and footage come from local models in the catalog when the
  user has a capable GPU.
- **Total freedom.** "Add your own" takes a GitHub link, a folder, a package, an MCP server command or URL, a plugin
  marketplace, or a skill the agent writes from a conversation. Users can also point Faber at other catalogs (a team's
  or a friend's). Own items go through the same security scan, which warns but never blocks.
- Updates are never automatic: diff, quarantine, previous version kept.
- The catalog is maintained by a weekly scout run, community pull requests and CI checks (license, link health,
  activity, smoke test, scan).
- A public repository publishes signed static files (no backend): the catalog, a nightly compatibility matrix built
  from recorded sessions against the newest CLI versions (users only move to green versions), a per-provider policy
  switch, and declarative driver descriptors so a CLI format change can be fixed without an app release. If GitHub is
  down, Faber keeps the last good manifest.

## Faber's own test system

One Rust program replaces the whole Node test suite. The reason is to avoid the CPU and RAM load that a heavy test
suite causes. Node stays a development tool for Vite, `tsc` and `oxlint`; it never runs tests and never ships.
The program is `faber-test`, run as `cargo t`.

1. **One light program.** It compiles with half the cores. Locally it runs only the tests that depend on the files a
   change touched. The full set runs on GitHub, on Windows, macOS and Linux.
2. **Fake CLIs that replay recorded conversations.** No test calls a real model or spends quota, and each runs in
   milliseconds. Changing one field of a recording must turn its test red.
3. **Screen tests run inside a hidden Faber**, opened only when needed. On Windows they drive the real app through the
   WebView2 DevTools port; the checks that need no such port run on every system.
4. **Unit tests never spawn processes, touch real git or call a real CLI.**
5. **The benchmark of ~40 real requests** that measures the goals against plain Claude Code and plain Codex before
   each version.

## Design and identity

The visual direction lives in [design.md](design.md). In short:

- One screen structure. Three themes (Dark Room, the default; Workshop; Workbench) and three presets (Morning,
  High contrast, Cobalt) are only sets of token overrides on a shared base.
- People can customize almost everything, and a custom look saves to a small file that can be shared. Three meaning
  colours are locked by the owner's decision: amber for "needs you", coral for Claude and violet for Codex.
- The symbol is "Curioso", a lime creature that peeks. The app icon is one dark icon in every Windows mode.

## License

Faber is under the Functional Source License, Version 1.1, ALv2 Future License (`LICENSE`, SPDX `FSL-1.1-ALv2`).
Faber is source-available, not open source. Anyone may use, change and share it for any purpose except a Competing Use,
as defined in `LICENSE`. Each version becomes Apache 2.0 two years after its release. What users create with Faber
belongs to them.

## Provider terms

- Anthropic allows a user to sign in to the unmodified Claude Code binary with their own subscription, including
  inside another platform. Faber accepts Anthropic's Commercial Terms before shipping and never offers a claude.ai
  login of its own.
- OpenAI: Codex runs through its official CLI and app-server with the user's own login. Faber asks OpenAI for written
  confirmation.
- Faber never drives a chat website (ChatGPT, Claude) in a browser or copies its login session: that breaks the
  providers' terms and puts the user's account at risk.

## Distribution

- GitHub Releases on every tag, downloaded directly: an `.exe` (NSIS) installer for Windows, a universal `.dmg` for macOS,
  AppImage and `.deb` for Linux. Windows comes first, and macOS and Linux compatibility is kept from now on. arm64
  Windows and `.rpm` come later.
- No paid signing for now. Windows shows a SmartScreen notice that the download page explains. The Microsoft Store and
  a code-signing certificate come back to the table when everything is complete.
- macOS builds are not notarized, so users allow the app once in System Settings. Faber updates itself there too. If a
  hand test on a real Mac shows that an unsigned update cannot install, macOS falls back to a notification about a new
  version.
- Later: winget and Flathub.

## Phases

The order of the phases says what gets specialized tools and tests first. Faber does everything from the start. Before
each version, the benchmark measures it, and the version ships only with the maintainer's OK.

| Phase | Scope | What comes with it |
| :-- | :--- | :--- |
| 1 | New base | the FSL-1.1-ALv2 license and these documents, Faber's own Rust test runner, the design system and themes, the identity, the shell layout and running list, the tray number and notifications, the first run and tutorial, Settings, and the CLI download written straight to disk. No public release: it is checked in the live app and a local installer build |
| 1a (R96) | Self-development MVP | the minimal Projects slice above ([spec](superpowers/specs/2026-10-12-self-development-mvp-design.md)); closes the project execution, handoff, stop and live-budget gaps needed to let Faber build Faber before Full Chat |
| 2 | Full Chat | the benchmark, visual answers, asking Faber about Faber, search, moving a chat to a project, Auto learning from approvals, the draft routed at most twice per message, reporting failed writes of conversation and project files, and closing the engine gaps of rules 2, 3, 5 and 6 |
| 3 | Projects with the lead agent | the lead agent, toolbox, 15 powers, modes, guardrails and big-work rules, the progress view, the decision list, the catalog, learning and the `.faber/` folder, and rules 4 and 7 |
| 4 | Excellent software | generalize the self-development workflow; nightly CLI version tests and the live manifest, shared project MCP infrastructure, and qualified department writers |
| 5 | First public version | |
| 6 | Image and design | creative drafts, blind judging and comparison views |
| 7 | Phone and routines | the phone as a remote control, messaging channels and routines |
| 8 | Video | |
| 9 | PC apps | |
| 10 | Mods and reverse engineering | |

Not planned yet: community themes listed in the catalog, code signing and the Microsoft Store, and animations for the
symbol.

### Non-goals

- Vendors other than Claude Code and Codex: no Gemini, Copilot, Cursor, OpenCode or ACP drivers.
- A path with no subscription, such as local models or Faber holding an API key.
- Team, admin and permission features.
- Driving a chat website or copying its login session, and using ChatGPT's connectors.
- Shipping any third-party tool, skill, model or MCP server inside the app or the repo.
- Telemetry that is on by default.
- A phone app that runs work. The phone is only a remote control.
- A separate help page. People ask Faber about Faber.

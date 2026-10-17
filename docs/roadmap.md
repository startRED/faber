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
- The router also says how the work can be checked: by a machine (a test, an exit code, a number to recount), at a
  glance, or blind (the person cannot tell a wrong answer from a right one). A blind medium request in code, math or
  research gets the hard bar, as math always did, because the model's own checking matters most where nobody else can
  check (done, `engine/src/route.rs`; measured in [design.md](design.md)).
- An answer whose last check failed, or whose code changed with nothing checking it, offers one step up: the same
  model one effort higher, else the next model at medium. It runs as a new branch, so a conversation's model and effort
  never change (done, `engine/src/nudge.rs`).
- **Still to build.** The table learns from what the user approves or redoes. The draft is routed at most twice per
  message while the user types; today it is routed at every pause.
- **Quota pacing** (built; [spec](superpowers/specs/2026-10-11-auto-quota-pacing-design.md)). Auto
  reads each window against its reset time, the weekday and the plan (detected at start and watched while open, on a
  shared account), and spends quota that would expire on quality: one rung up when a vendor has spare quota.
- The router is measured too: on the benchmark of real requests, its picks must match always using the strongest
  model while spending less quota.

## Two areas

- **Chat** works like claude.ai and ChatGPT, and must beat them. It has many conversations. Each one works in a hidden
  workspace inside the app where the model runs code, draws charts and creates files, or in a folder the person picks
  before its first message (R107). Files show up in the conversation with Save and Show, and they stay in the workspace
  until the user saves them; "take to a project" and moving a conversation to a project are planned, not built. "Don't
  ask", the default, gives full access (R121); "Ask before commands" asks before file changes and commands, except plain
  reads and what the person already allowed in the tool's own settings. The user picks any connected
  model or Auto, and every answer shows the model and, in Auto, the reason in one line.
- **Projects** holds many git folders. The person gives a project a goal and the lead agent's mission does it in a
  separate copy; today that is the self-development slice of R96 (one task per mission), and no direct conversation
  starts inside a project. Moving a Chat conversation to a project with its full context is planned.

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

### Running many workers

Taken from Claude Code Projects and pushed further (AI proposal, not reviewed; research in
[projects-orchestration.md](research/2026-10-14/projects-orchestration.md)).

- **The lead routes every message.** A quick question is answered in place; new work goes to a new worker or to the
  worker already in that area; several tasks in one message become several workers. It can list suggested workers
  with "start all".
- **The lead never reads the whole history.** It works at low effort from worker reports, recent turns and the
  `.faber/` memory, so a project can run for weeks.
- **A brief in five parts** goes to every worker, written by Faber, not by the user: what the project is for, where
  the work happens, how a worker checks its work, what to do when something is missing, and what needs approval.
- **A new project starts with proposals.** Faber reads the recent work and proposes the first tasks; the first worker
  only reads and proposes next steps.
- **Missing-item gate.** A worker finishes through a report with required "assumed" and "could not reach" fields. A
  missing login, tool, file or secret, or a stand-in (a mock, a fake key, an invented path), keeps the step waiting
  with the missing item named, and "done" is refused. The user's answer goes into the brief, and every new brief is
  checked against past gaps first. The reviewer from the other vendor checks "assumed" against the diff.
- **A canary before a batch.** When the lead fans out similar workers (one per repository, file, chapter or
  thumbnail), one runs first and passes its proof and a review from the other vendor; its mistakes become rules in
  the brief, then the rest start in parallel. Batches of three or fewer, or with a very cheap proof, skip it.
- **Workers leave a written state and exit.** A worker's lease renews only with evidence of progress. When it lapses
  or the work ends, the worker writes its state (step, branch, next step, proof, files) to `.faber/` and its process
  closes. A follow-up resumes the session while the one-hour cache is warm and starts a fresh one from that state when
  it is cold, showing both token estimates. The same file serves vendor switches and crash recovery.
- **Workers claim their files (R155).** Before it writes, each worker states the files it will change; Faber refuses a
  change outside the claim or a claim that overlaps another worker's, and the reviewer sees the claim. It comes with
  missions of more than one task, as Faber's own lanes already do (R154).
- **Limits in plain words become real limits.** "At most two at a time" or "ask before starting" is stored as a
  setting the engine enforces, not a hint for the model. New workers have a daily cap.
- **After delivery the worker keeps watching.** A worker that opened a pull request fixes failing checks and review
  comments until it lands. Its row offers the next step as a plain button ("Fix the failing check", "Publish"), and
  publishing still follows guardrail 3.
- **The progress view adds "Waiting on you" and "Landing"** (approved, waiting to merge), and a worker idle for a week
  is resolved by itself.
- **A library per project** lists the files the user added and the files workers produced.
- **One question explains a stall:** for every worker, what it was asked, what it assumed, what it could not reach and
  what it waits on.

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
5. Another model reviews the work. In code, the writer never reviews its own work. The reviewer is preferably the other
   vendor; when its quota is poor or only one vendor is connected, a fresh read-only session of the same vendor reviews
   (R146; today's coordinator still requires the other vendor). Creative work gets two judges from different vendors
   when both are connected.
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

### Coherence of a project's work (R146)

A mission keeps a project as coherent as the agents that build Faber keep Faber (R145). Planned, not built:

- **A project map built from the code.** Extractors list what the project has (settings, routes, commands, texts,
  exports) and stay tied to the files they came from; no AI-written overview. Roles get the index and look details up.
- **A contract before writing.** The Lead names what the change touches, the closest thing that already exists, and
  `Extends:` it or `New:` with why it does not fit; the Writer starts only once the contract exists.
- **Faber decides "done".** After the Writer's turn Faber runs the project's checks and the map check itself; a Writer
  cannot declare its own work done.
- **Short check output (R155; reading built for Cargo, tsc and oxlint, pytest, jest or vitest, eslint and go test
  next).** The roles read only what failed: each failure with its file and line, the tool's
  suggested fix when it has one, measurements and a one-line summary. The whole output stays in the mission's log, and
  a format Faber does not know falls back to its last lines. Faber's own runner cut a passing run from about 34k tokens
  to about 50 this way (R154).
- **Lessons need the owner's yes.** A mission's retrospective proposes rule or card changes with their evidence, and
  each becomes a rule only after the owner's yes.
- **Sized by risk.** A small change in one area gets the Writer and the checks; a change across areas gets the whole
  chain.
- **The reviewer** follows guardrail 5: the other vendor when it can, a fresh read-only session otherwise. It reviews
  by the rules of [product-rules.md](product-rules.md) (R147): an empty review is a pass, only a break for a person, for
  data, for security or for a written rule blocks, a fix needs a failing test or a repro, and each round gets the
  findings already decided so the loop ends.

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
- In a project, the agent's own work shows as pieces too: a site it opened, the app it is running, a test result, a
  changed file, each with a small picture and Open ([Watching the agent work](#watching-the-agent-work)).

## Watching the agent work

The owner's ask (2026-10-16): the person sees every part of the work happening and steps in as easily as reading a
chat. Steps 1 and 2 are built: each tool reads as a verb with what it acts on (one verb per tool; grouped phrases such as "Changing 3 files" and the time per subagent are not), and subagent rows are the steps of R161. The rest is planned. The design is an AI proposal, not reviewed, picked from a divergent pass (30 ideas under five
frames) and checked against today's code. It fills the steps of R161 and never becomes a board or an agent map.

**In order, each one extending what exists:**

1. **Words for every step (now, Chat too).** Today the work log names only Codex's four item kinds; Claude's tools show
   their raw names ("Read", "Task"), and the open log is a flat list (`ui/worklog.ts`, `ui/Thread.tsx`). Each tool
   gets a plain present-tense phrase built from its input with no model call ("Reading akitaonrails.com/en/posts",
   "Running the tests", "Changing 3 files"): the engine-side event stays as it is, and `worklog.ts` maps it to a key
   with arguments that each language writes by hand. The status line that already shows the newest step becomes this
   living sentence.
2. **Subagents as rows inside the step (R161 step 2).** Claude's stream marks every event of a subagent with
   `parent_tool_use_id`, which `engine/src/claude.rs` drops today, and the Task/Agent call already carries the whole
   brief in its input. The engine keeps that mark on `Tool`, `Text` and `ToolResult`; the screen groups a child's
   events under its Task step, which opens into one row per child: the short name the agent gave it, its living
   sentence and its time. Past three rows the rest fold into "+2 more", and while nothing is stuck the whole split
   reads as one sentence ("5 collectors reading 5 sites"). Codex's own subagents show the same way (R167).
3. **The side chat lives in the right column.** The running column (`ui/App.tsx`, `ui/Running.tsx`) is already the one
   place on the right; clicking a child row opens that child there as a normal conversation, with a back arrow to the
   running list, so there is no second column. Its first message is the whole brief the main agent sent, word for word,
   then the child's messages, steps and results as they arrive, drawn with the conversation's own message parts and
   loaded only while open. A CLI subagent cannot be messaged while it runs, so its box writes to the main agent about
   that step and says so.
4. **Facts, not a verdict.** When a child ends, its row shows what it was asked, what it did (files changed, checks run,
   steps) and anything it touched that the brief never named. The main agent grading its own helper is not trusted:
   each brief ends with a "done when" list, and the row ticks each line the report meets.
5. **Helpers talk back (R161 step 3).** A helper is a real Faber session on the other vendor, so in the side chat the
   box writes to that child, and the main agent gets the message before its next step. A finished subagent offers
   "Continue with a helper".
6. **Stuck shows only when it matters.** The engine notices a child repeating the same action, failing the same check
   twice, or silent longer than its kind of work allows; a child waiting on a question is "waiting for you", never
   stuck. The main agent hears first and tries once to retry, narrow the piece or hand it to the other vendor. Only if
   that fails does the row turn amber with "Look" and "Tell it…", and only then is the person notified. It ships once
   the recorded fixtures show no false amber. Built (R-next): neither CLI can be told about its subagent while the main agent waits for it, so
   the main agent's chance is the retry the thresholds leave (a check shows only after it failed three times).
7. **Faber's own browser and app pane (R161 step 4, the largest piece).** Nothing exists yet: the shell has one
   webview. The agent or a child opens websites in a pane that belongs to Faber and runs the app it builds beside the
   chat, through a Faber tool the CLI calls. A card in the reply ("Site open in the browser", with a picture and Open)
   is the way in, next to the file and question cards that already exist. The pane has one owner at a time: "Take over"
   stops the agent at its next action, the person clicks and types, and "Let it continue" hands it back with a short
   note of what the person changed. Logins, captchas and payments are always the person's turn ("Your turn" on the
   card), never the agent's (guardrails).
8. **Recorded, not streamed.** Each pane keeps a log: a small picture at each page change or agent action, with the
   action that caused it, typed secrets masked and password fields blurred. Watching live is playing the end of that
   log, coming back later is scrubbing it, and "Take over from here" starts from the latest picture. Only the pane the
   person opened is a real live view; every other child is a still picture that steps forward. The log follows the
   attachments of a project conversation (`<data dir>/attachments/<conversation id>/`): Faber's own data, never the
   project folder (R35), never in an export, trimmed by size and deleted with its conversation. "While you were away"
   sums it up in a few pictures with one sentence each.
9. **The view fits the work.** The owner's ask (2026-10-16): the person watches each agent work in whatever way makes
   that work easiest to follow. The rule is the goal; the examples below are starting points, not a spec, and the
   references (Google Stitch, Claude's own browser pane) are for the feeling, never the look.
   - Browsing: the page live as the agent moves through it, its pointer and what it clicks or types, and the line of
     what it is looking for.
   - Building a screen: the screen filling in as each change lands (the project's dev server reloads on its own), the
     changed part outlined for a moment, and a named pointer per subagent that opens it in the side chat.
   - Changing files: the few lines that changed, added and removed. Running checks: a count that ticks up, green or red.
   - Researching: the sources it opened and the one it reads now. Long or hidden work: the living sentence and the time.

   Each view is built from the agent's own calls and the pane's log, with no model call and no extra process; only the
   view the person opens is live. Reduced motion keeps the outlines and tags without movement; during a game nothing
   moves.

**Across all of them:**

- **Quiet during games.** The tray already knows when a game is in front (`src-tauri/src/tray/native.rs`, private
  today); that check becomes one the screen can ask. While it holds, pictures stop, rows stop repainting and events wait
  as one summary for the person's return. Nothing opens a window or takes focus.
- **Cost first.** Faber idles near 110 MB today against the 400 MB test (R125). Steps 1 to 6 add no process and only
  the rows on screen. A second webview is the big cost: one live view at a time, and before it is built a Windows
  spike measures it (a hidden child webview driven by the engine, a small picture after each of 20 actions: stale or
  blank pictures, memory, processor, and any focus flash while a game is in front).

Parked: a floating picture over the game (takes the screen), a strip board with columns (a board, R161), a shortcut
palette (a second way to do what the composer does), a layout that rearranges itself when the person looks away, and an
approval counter every result must pass before it enters the project.

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
   cache, and a change of tools or system prompt invalidates everything after it. A change of effort kept the Claude cache
   through `--resume` and cost Codex nothing measurable (13/10/2026, [design.md](design.md)). Facts that change travel as
   text inside the next message, never as a system-prompt edit.
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
| 2 | A session may start on the account's default model or effort (Chat passes `--permission-mode` for each permission choice; Projects has no direct sessions, and mission roles run isolated) | Phase 2 |
| 3 | Auto's move up changes the model inside a conversation | Phase 2 (check the hand-over) |
| 4 | There is no lead agent yet, so there is no hand-over | Phase 3 |
| 5 | Interrupt waits 5 s for the CLI to stop (`engine/src/proc.rs`), and only mission roles are watched for 2 min of silence; a Chat answer that goes silent is not, and off Windows stopping ends the CLI alone, not its tree (on Windows its job object ends the whole tree) | Phase 2 |
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
  complete; the end-to-end test fails above 400 MB (R125). The limit is a safety net: each change still aims at the
  lightest and most responsive result it can reach. Anything that keeps a
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

- **Faber itself** updates itself. Today it downloads a signed update in the background and installs it on quit or, on
  Windows, at the first moment no conversation, mission or CLI install runs and the window is hidden (R118). Installing
  when Faber opens (R141) is not built yet.
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
- Proposal, not built (R148): the same weekly scout also looks for new practices on coherence, review and harness
  quality, judges each against the project's rules, and asks the owner a yes or no before any becomes a rule. It then
  replaces the owner's own weekly scout that does this for Faber today.
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
6. **Still to build: faster and lighter than TL-ADE's suite, measured fairly** (R113, R115, R116, owner;
   [spec](superpowers/specs/2026-10-14-test-system-rework-design.md),
   [plan](superpowers/plans/2026-10-14-test-system-rework.md)). A job-object meter measures both projects the same way:
   the warm suites, 8 matched scenarios and 7 real-work tasks, never compilation. The floor is 5x less time and
   processes and 3x less memory and CPU, with no ceiling; no test is weakened. Steps: one engine test program, the
   `rust-lld` linker, lighter debug info and below-normal priority (landed 2026-10-14), then event waits,
   parallel lanes, skipping unchanged builds and a shared running app for the screen scenarios.
7. **The build folder has a budget** (R106). Every worktree builds into a folder of the main checkout's `target`, and
   Cargo never deletes anything there, so it grew to 123 GB and once filled the disk. `cargo t` sweeps it at start, at
   most once an hour: folders of removed worktrees and builds replaced more than a day ago go, and above 60 GB
   the least recently used worktree folders go too. A folder in use is never touched
   ([design](design.md#build-folder-budget)).

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
- Every user-facing change ships by itself as a preview, and a stable follows weekly (R130, R156). When CI goes green on
  `phase-1-base` and a `feat`, `fix` or `perf` commit that touches the shipped app (`ui/`, `engine/`, `src-tauri/`,
  `package.json`, `vite.config.ts`) landed since the last tag, `auto-release.yml` tags `vX.Y.(Z+1)-preview.N` and starts the
  release as a GitHub pre-release. People who turned on "Receive previews" update to it through the built-in updater, from
  the fixed `preview-channel` release's `latest.json`. Every Wednesday `promote-stable.yml` rebuilds the newest preview
  published at least a day ago (with a complete manifest) as `vX.Y.Z`, the release `/releases/latest` points at, which
  everyone else reads (it can also be run by hand with a preview tag for an urgent fix). A finished area still gets its
  minor bump by hand. The tag is the version; the
  release build writes it into `package.json`. Semver pre-releases are accepted by every bundle (NSIS, DMG, AppImage,
  deb); an MSI would reject them and is not built. Every release carries `SHA256SUMS.txt` and build provenance
  (`gh attestation verify <file> --repo startRED/faber`). `scripts/channel.mjs` holds the rules and its tests run in CI.
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
| 1 | New base | the FSL-1.1-ALv2 license and these documents, Faber's own Rust test runner, the design system and themes, the identity, the shell layout and running list, the tray number and notifications, the first run and tutorial, Settings, and the CLI download written straight to disk. Every user-facing change already ships as a patch release (R130) |
| 1a (R96) | Self-development MVP | the minimal Projects slice above ([spec](superpowers/specs/2026-10-12-self-development-mvp-design.md)); closes the project execution, handoff, stop and live-budget gaps needed to let Faber build Faber before Full Chat |
| 2 | Full Chat | the benchmark, visual answers, asking Faber about Faber, search, moving a chat to a project, Auto learning from approvals, the draft routed at most twice per message, reporting failed writes of conversation and project files, and closing the engine gaps of rules 2, 3, 5 and 6 |
| 3 | Projects with the lead agent | the lead agent, toolbox, 15 powers, modes, guardrails and big-work rules, the rules for running many workers, the progress view, watching the agent work (living sentences, side chat, Faber's browser and app pane), the decision list, the catalog, learning and the `.faber/` folder, and rules 4 and 7 | |
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

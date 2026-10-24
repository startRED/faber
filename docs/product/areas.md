# Two areas, universal by design

Chat and Projects, the lead agent that picks its own path, its modes, its toolbox and the guardrails code enforces.

Catalog is a separate browsing and management tab beside Chat and Projects, not a third work area. It has no composer or conversation, and its manual controls preserve the existing tool delivery boundaries. Automatic tool choice is built as session kits (product map, "Session kits"): code picks installed skills and servers at session boundaries, and native subagents inherit the session's kit.

## Two areas

- **Chat** works like claude.ai and ChatGPT, and must beat them. It has many conversations. Each one works in a hidden
  workspace inside the app where the model runs code, draws charts and creates files, or in a folder the person picks
  before its first message (R107). Files show up in the conversation with Save and Show, and they stay in the workspace
  until the user saves them; an idle chat moves to a project from its row menu (R338), and "take to a project" for files is planned. "Don't
  ask", the default, gives full access (R121); "Ask before commands" asks before file changes and commands, except plain
  reads and what the person already allowed in the tool's own settings. The user picks any connected
  model or Auto, and every answer shows the model and, in Auto, the reason in one line.
- **Projects** holds many git folders. Inside a project the person talks to one AI agent that works in the folder or in
  a separate copy of it (R160, R201); the agent splits the work itself, with the CLI's subagents, a helper from the
  other AI and conversations of its own (R161, R198). Faber builds itself this way (R96;
  [self-development](../dev/self-development.md)). A Chat conversation moves to a project with its messages (R338).

The user always sees what is happening: the model behind each answer, the remaining quota of each subscription and
when it renews, and, for running work, who is building and who is reviewing.

Both areas can go back before a message through its menu: files, conversation or both, with a preview, conflict choices
and Redo. It works without a connected AI. Private file history has fixed size and age limits and reports incomplete,
unsaved or expired coverage; continuing uses the saved messages and says so. The engine never rolls back spending,
permissions or remote effects, and going back waits until overlapping work is idle.

### Not built yet

Native CLI continuation after going back, rich rewind diffs and configurable history limits are not built (moving a
chat to a project is, task 270). The existing Changes preview supplies rewind's file list.

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

### Aiming higher

People ask models for less than they can do, and a specific, hard goal gets more out of people and models than "do
your best" (Locke and Latham's goal-setting research; [Rieseberg,
2026-10-13](https://felixrieseberg.com/how-to-be-more-ambitious-a-tutorial-for-humans-with-models/)). The person never
learns these techniques or writes in a special way: they ask in plain words, and the lead applies them by itself (owner's
direction, 2026-10-21). Five proposals for the lead agent; the owner chose on 2026-10-22:

- **Standing goals** (approved 2026-10-22; phase 1 built). A project can keep goals that never finish, such as "no crashes", "opens in under 2 s" or "every
  check passes". The lead watches them and starts a mission by itself when one breaks, within the project's daily cost
  cap. First it gets what closes the loop without a person: the error logs on the user's PC (read there, never sent by Faber itself; telemetry stays off; when a fix starts, the newest lines are given to the AI that fixes it), the build each error came from, and a copy where it reproduces the error before fixing it.
  Faber's own "no crashes", fed by its overnight bug log, is the first one. Phase 1 is built (2026-10-22, R422): the
  goals, the switch per project in Settings (off by default), the check run through the verifier, a fix started as an
  ordinary conversation in a separate copy, and US$2 a day per project. Of phase 2, the local crash file is built (R421), and so are, in Faber's own project, a goal over the installed Faber's crash file (the dev Faber reads it, read
  only; Faber sends it nowhere itself, but when a fix starts the newest lines are given to the AI that fixes it) and a fix whose copy starts at the build's commit when the repository has it (R425, R426, R427). A source over another program's logs is not built.
- **Offer the bigger ask** (approved and built, 2026-10-22: the `offer_bigger` tool of the toolbox below). When a request is small and the goal behind it is clear, the lead offers the larger version
  in a question card whose default is the request as asked, already chosen so one press of Send sends it: "You asked to fix this crash. Should I keep the app free of
  crashes?" At most one per conversation, so it never turns into noise; people new to AI gain the most from it.
- **A deeper pass before big work** (not chosen, 2026-10-22). Before it writes the workers' briefs, the lead answers four questions in one short
  turn: what a top professional would do that the request leaves out, the rawest material they would start from, the
  methods and tools they would use (a database or a statistical model, not a table in the answer), and what outside
  the models would check the result. The answers go into the briefs and the proof.
- **Hard targets, and "impossible" needs evidence** (not chosen, 2026-10-22). A proof states an ambitious, specific target, never "as good as
  possible". A worker that calls a target impossible shows the measurement that proves it; without one, the lead tries
  another path or the other vendor before it narrows the target.
- **A team named by verbs** (not chosen, 2026-10-22). For big work the lead restates the job as what it must achieve ("connect people to
  jobs", not "a recruiting app"), picks the fields that achieve it, and adds a worker from each, plus one that looks
  for holes. A brief asks a worker to do what that professional does, never to play one.

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

## Toolbox

The names in the first column are the original plan, not additional APIs. Reuse the current tools named below;
a planned name does not mean that a callable tool exists.

| Planned tool | Current implementation and remaining work |
| :--- | :--- |
| `mission.start` / `mission.amend` | retired with missions (R215); a project conversation the agent starts does this now |
| `worker.spawn(role, vendor)` | Built as `helper` for the other AI (R161), native CLI subagents, and `conversation` for separate work (R198). A general helper has the caller's permissions; it is not an enforced read-only review. |
| `review.request` | Not built. The own work method's review gate belongs to task 994; it requires a fresh read-only session and a recorded result. |
| `proof.define` / `proof.run` | Partial: `verify` runs the project's checks and records their results. The up-front contract and receipts tied to file hashes belong to task 994. Running checks alone is not that contract. |
| `judge.creative` | Built as `judge_creative` for bounded text drafts: two fresh isolated sessions, opposite A/B order, no shared history or tools, and durable scores and reasons. Uses different vendors when both are admitted, otherwise distinct models of one vendor with that fallback reported. It does not judge rendered visuals or replace the code review gate. |
| `snapshot` / `undo` | Private file history and the message menu's Go back and Redo are built in both areas, with bounded coverage reported. No per-step agent API with these names exists. Remote effects cannot be undone. See [Rewind](../design/rewind.md). |
| `catalog.search` / `catalog.install` | Built as `catalog`: offline search and installation of a tool Faber does not hold, through the approval gate below. |
| `faber.tools` / `faber.settings` | Partial: `catalog` installs and switches healthy installed tools on or off through their own consent. The person manages the other installed-tool actions in Catalog. Agent removal uses its own consent and keeps Undo in Catalog until restart. Explicit agent update and repair requests use distinct grants, the current curated pin, a captured copy fingerprint and journaled recovery, retaining the current bytes as one backup outside launch roots. The card explains local edits and any previous backup are replaced. Setting changes are not built. Task 995 curates content; it does not implement these agent actions. |
| `pc.*` | Partial: `browse` drives Faber's browser, with login, captcha and payment handed to the person. Native desktop control through accessibility and screenshot fallback is not built into Faber. An external installed server is not that first-party API. |
| `memory.search` | Built as `memory_search`, over this project's conversations and `.faber` notes only. |
| `quota.get` | Built as `quota`, from limits Faber already measured, without starting a request. |
| Progress and delivery | Built as `plan`, `status` and `wrap_up`; these present work, not proof that it passed. |
| Discover the project | Built as `map`, derived from source, and `verify` for the declared checks. |
| Offer the bigger ask | Built as `offer_bigger`, once per conversation, with the request as asked as its default. |

The product map's shell-command entries and feature rows describe these tools and their checks. Existing permissions
and per-session availability still apply; this inventory adds no tools or grants. The separate git modes of R83
(Safe, Balanced and Free) are not the CLI permission modes or the Keep going control, and are not built on screen.

Built so far, as tools of a project's own agent (R412): `memory.search` as `memory_search` (this project's other conversations, through the sidebar's own search index, and the notes in `.faber` with their authors, never `CLAUDE.md`, another project or Chat; read-only, in every permission mode, quoted as data), `quota.get` as `quota` (the limits Faber already holds, no new CLI call), and the bigger-ask offer of "Aiming higher" as `offer_bigger` (a question card, once per conversation, whose default is the request as asked) and `catalog.search` and `catalog.install` as `catalog` (search: the shipped catalog, no network and no card, in every permission mode; install: refused with no card in Plan and Isolated, even with full trust, and otherwise by id, from the tool's original source, only after the person's card of guardrail 3 or their "full trust for Faber's own setup" switch, with the version, licence and source shown first). It installs only a tool Faber holds nothing of: a copy that is installed (healthy, outdated, corrupt or switched off) or removed with its Undo copy still there is left as it is, and the agent is told the person does it in Catalog. Each install is logged in `self-changes.jsonl` as the decision, written before anything is downloaded, and its result; the newest lines stay once the log reaches 200 KB. A decision that cannot be written stops the install however it was allowed, what Faber holds of the tool is checked again under the store lock before anything is written, and a second install of one id while one runs is refused. The same catalog tool switches healthy installed copies on or off through its own consent and removes an installed catalog id through separate removal consent or full trust. Removal preserves Undo in Catalog until Faber restarts and retains tools for ongoing conversations. Install and switch grants do not authorize removal; Plan and Isolated refuse it. On the person's explicit request, update and repair use their own grants and the current curated pin, revalidate the captured copy, and retain its bytes as one backup outside launch roots (R443). They replace local edits and any prior backup, as the card explains; ongoing conversations keep their tools. Changing a setting from the agent is not built. Together the three tools add about 400 tokens to every request of a project agent, and `offer_bigger` with its line of the prompt about 350 more. See "Faber's AI limits", "Faber's catalog search and install", "Faber's project memory search" and "Offer the bigger ask" in [the product map](../product-map.md).

The `judge_creative` definition is also offered with the project toolbox. It spends quota only when called; its prompt-token overhead has not been measured. The initial implementation compares text drafts only; rendered-visual judging remains unbuilt.

## Guardrails enforced by code, never by the model

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
   work above; built for a project agent's plan estimate, not yet for missions). Unattended missions have a fence: whatever hits it is parked with a note and the rest continues.
8. Whatever shows up on screen, in a page or in a file is data, never an instruction.
9. Nothing third-party ships with the app. Telemetry is off by default.

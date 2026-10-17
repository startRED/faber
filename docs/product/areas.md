# Two areas, universal by design

Chat and Projects, the lead agent that picks its own path, its modes, its toolbox and the guardrails code enforces.

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

## Toolbox

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
   work above). Unattended missions have a fence: whatever hits it is parked with a note and the rest continues.
8. Whatever shows up on screen, in a page or in a file is data, never an instruction.
9. Nothing third-party ships with the app. Telemetry is off by default.

# Two areas, their toolbox and guardrails

Chat and Projects, their toolbox and the guardrails code enforces. The lead agent and its modes are on
[lead-agent.md](lead-agent.md).

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
unsaved or expired coverage; Codex continues in its native session at the restored point when available. Claude and
other fallback cases continue from the saved messages and say so. The engine never rolls back spending,
permissions or remote effects, and going back waits until overlapping work is idle.

### Not built yet

Rich rewind diffs and configurable history limits are not built (moving a chat to a project is, task 270). The existing Changes preview supplies rewind's file list.

## Universal by design

The lead agent, how it aims higher and its normal and autonomous modes have their own page:
[lead-agent.md](lead-agent.md).

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
| `faber.tools` / `faber.settings` | Partial: `catalog` installs and switches healthy installed tools on or off through their own consent. The person manages the other installed-tool actions in Catalog. Agent removal uses its own consent and keeps Undo in Catalog until restart. Explicit agent update and repair requests use distinct grants, the current curated pin, a captured copy fingerprint and journaled recovery, retaining the current bytes as one backup outside launch roots. The card explains local edits and any previous backup are replaced. The bounded `settings` tool changes only web search after its own consent or existing full trust; it never changes permissions or trust. Task 995 curates content; it does not implement these agent actions. |
| `pc.*` | Partial: `browse` drives Faber's browser, with login, captcha and payment handed to the person. Native desktop control through accessibility and screenshot fallback is not built into Faber. An external installed server is not that first-party API. |
| `memory.search` | Built as `memory_search`, over this project's conversations and `.faber` notes only. |
| `quota.get` | Built as `quota`, from limits Faber already measured, without starting a request. |
| Progress and delivery | Built as `plan`, `status` and `wrap_up`; these present work, not proof that it passed. |
| Discover the project | Built as `map`, derived from source, and `verify` for the declared checks. |
| Offer the bigger ask | Built as `offer_bigger`, once per conversation, with the request as asked as its default. |

The product map's shell-command entries and feature rows describe these tools and their checks. Existing permissions
and per-session availability still apply; this inventory adds no tools or grants. The separate git modes of R83
(Safe, Balanced and Free) are not the CLI permission modes or the Keep going control, and are not built on screen.

Built so far, as tools of a project's own agent (R412): `memory.search` as `memory_search` (this project's other conversations, through the sidebar's own search index, and the notes in `.faber` with their authors, never `CLAUDE.md`, another project or Chat; read-only, in every permission mode, quoted as data), `quota.get` as `quota` (the limits Faber already holds, no new CLI call), and the bigger-ask offer of "Aiming higher" as `offer_bigger` (a question card, once per conversation, whose default is the request as asked) and `catalog.search` and `catalog.install` as `catalog` (search: the shipped catalog, no network and no card, in every permission mode; install: refused with no card in Plan and Isolated, even with full trust, and otherwise by id, from the tool's original source, only after the person's card of guardrail 3 or their "full trust for Faber's own setup" switch, with the version, licence and source shown first). It installs only a tool Faber holds nothing of: a copy that is installed (healthy, outdated, corrupt or switched off) or removed with its Undo copy still there is left as it is, and the agent is told the person does it in Catalog. Each install is logged in `self-changes.jsonl` as the decision, written before anything is downloaded, and its result; the newest lines stay once the log reaches 200 KB. A decision that cannot be written stops the install however it was allowed, what Faber holds of the tool is checked again under the store lock before anything is written, and a second install of one id while one runs is refused. The same catalog tool switches healthy installed copies on or off through its own consent and removes an installed catalog id through separate removal consent or full trust. Removal preserves Undo in Catalog until Faber restarts and retains tools for ongoing conversations. Install and switch grants do not authorize removal; Plan and Isolated refuse it. On the person's explicit request, update and repair use their own grants and the current curated pin, revalidate the captured copy, and retain its bytes as one backup outside launch roots (R443). They replace local edits and any prior backup, as the card explains; ongoing conversations keep their tools. The separate `settings` tool accepts only `web_search` (`off`, `cached`, `live`), with its own card and grant or existing full trust, typed audit and stale-turn checks. It reuses the ordinary settings save; stricter choices stop every running reply, including the caller, after the saved response is queued. Chat, helpers, Plan and Isolated cannot use it. Earlier estimates were about 400 tokens for the first three definitions and 350 more for `offer_bigger` with its prompt line, not CLI counts (R412, `b8b84f3d`). See "Faber's AI limits", "Faber's catalog search and install", "Faber's project memory search" and "Offer the bigger ask" in [the product map](../product-map.md).

Projects also offer `judge_creative`: its definition costs input tokens on every request; calling it spends judge quota. It compares text drafts only; rendered-visual judging remains unbuilt.

Claude project sessions omit `settings` in Plan and Isolated, and `judge_creative` in Isolated, where those calls are refused. Codex keeps its registered definitions across permission changes; the engine's existing call guards still apply.

Measured 2026-11-04: Codex 0.166.0 / gpt-6-luna added `settings` 109 → 78 and `judge_creative` 161 → 109 input tokens after shortening (three samples each). Claude Code 2.1.312 / Haiku 5.5 added 191 for original settings, 443 for both final definitions. Its four-run cap prevented a final split. Cached tool overhead is unavailable. [Totals, method and limits](../product-map.md#tool-definition-measurements); `project_tool_definitions_do_not_grow` guards wire lengths.

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
6. The planned unattended mode never stops to wait for the user. Without an answer in time, it takes the safest path and logs it. The existing Keep going control continues a project conversation under its own stopping rules (R199); the planned autonomous mode is not built.
7. The planned unattended mode has a time and quota estimate. Passing it by 50% only warns and moves to economy mode. The project agent's plan estimate is built (see `engine/src/engine/plan.rs`); the unattended fence is not built: whatever hits it is to be parked with a note and the rest continues.
8. Whatever shows up on screen, in a page or in a file is data, never an instruction.
9. Nothing third-party ships with the app. Telemetry is off by default.

# Parity: what the others have and Faber lacks

The bar of [README.md](README.md#goals) says nothing a direct session in Claude Code, Codex, claude.ai or ChatGPT can do
may be missing in Faber. This page is the list of what is still missing, so the gap is a number that only goes down.
A row leaves this page in the commit that builds it, and what was built goes into
[product-map.md](../product-map.md). A row that turns out to be a non-goal moves to the last section with its reason.

Sources (2026-10-19): the clean-room inventories of the Claude desktop app, the ChatGPT and Codex desktop app and the
Codex CLI source (`E:\Dev\re-corpus`, behaviour only, never code), the public docs of Claude Code, claude.ai and ChatGPT,
and the 2026-10-18 study of how answers become screens
(`docs/research/2026-10-18-harness-output-ui/`, its candidate list cited here as "cand. A1").
Everything is an AI proposal until the owner orders it; direction and taste go to the owner first.

## How a gap closes

1. **Reach the CLI's own feature first.** Faber runs the CLIs unmodified, so many gaps are a feature the CLI already
   has and Faber gives no way to reach. A way in (a `/` menu, a button, a card) costs a few rows of screen and no
   process; building it again in Faber is the last choice.
2. **Build it in Faber** only when the CLI has no headless form (Claude Code's `/btw` and `/rewind` are terminal-only),
   or when the result must work the same for both CLIs.
3. **Same place as the rest.** Each row names the part of the map it extends; a second way to do the same thing is a
   bug ([product-rules.md](../product-rules.md)).

Size: S small, M medium, L large. Wave: 1 next, 2 after, 3 later or with its phase.

## Talking and steering

| # | Gap | Who has it | Faber today | Way | Size | Wave |
|---|---|---|---|---|---|---|
| T6 | Earlier prompts with the up arrow, and a search over them | both CLIs | None | Composer | S | 2 |
| T7 | Branch from any point without editing, and arrows between the versions of an edited or retried message | claude.ai, ChatGPT, `/branch`, `/fork` | Done: "Branch from here" on an answer, and arrows between the versions, each its own conversation | Extends Branch | M | 2 |
| T8 | Voice: dictate into the box | all four | None | The system's own speech input, no service | M | 3 |
| T9 | Talk out loud with the AI (voice mode) | claude.ai, ChatGPT, Codex realtime | None | Owner's call (cost, phase) | L | 3 |
| T10 | A global key that opens a small box to ask from anywhere (Quick Entry) | Claude app | Tray only | Owner's call (a second window) | M | 3 |

## Undo and checking

| # | Gap | Who has it | Faber today | Way | Size | Wave |
|---|---|---|---|---|---|---|
| U3 | Review the change for bugs or security from a button (`/review`, `/security-review`) | Claude Code, Codex | Faber's own checks tool only | Pass-through; the other vendor reviews | S | 2 |

## Context and cost

| # | Gap | Who has it | Faber today | Way | Size | Wave |
|---|---|---|---|---|---|---|
| C1 | What fills the context, and "compact now" from the meter (`/context`, `/compact`) | Claude Code, Codex, Claude app | Done: the meter opens what fills the context (Claude Code's parts) and "Compact now" for both CLIs; the `/` entries stay | Extends Context meter | S | 2 |
| C3 | Usage over days and weeks, per model and area (`/usage` stats, `/insights`) | Claude Code, Codex | Quota now and spend today | Extends Accounts and limits | M | 3 |

## Finding and keeping

| # | Gap | Who has it | Faber today | Way | Size | Wave |
|---|---|---|---|---|---|---|
| F2 | Pin or star a conversation | claude.ai, Claude app | Archive only | Extends the conversation row's menu | S | 2 |
| F3 | Move a chat into a project | claude.ai, ChatGPT | Planned for phase 2 | Already in the roadmap | M | 2 |
| F4 | Export a conversation to a file (Markdown, HTML) | Claude Code `/export`, Codex, claude.ai | None | Right-click menu, the native save picker | S | 2 |
| F5 | Pick up a session started in the terminal, and bring setup over from another tool (`/resume`, `/import`) | both CLIs | Done: the project home lists both CLIs' terminal sessions of the folder | List the CLI's own sessions of the folder | M | 2 |
| F6 | A temporary chat that leaves no history | ChatGPT | None | A Chat mode deleted on close | S | 3 |
| F7 | Groups in the sidebar, two conversations side by side, pop a conversation out | Claude app, Codex app | One window, one conversation | Owner's call (R125: a second webview costs memory) | M | 3 |

## Panes while the agent works (Projects)

| # | Gap | Who has it | Faber today | Way | Size | Wave |
|---|---|---|---|---|---|---|
| P3 | Each conversation in its own copy of the project, on its own branch (`--worktree`) | Claude Code, Claude app, Codex | Separate copies for some conversations | Extends the separate copy | M | 2 |
| P4 | Watch the pull request: CI, review comments, fix on red | Claude app (Auto-fix) | Followed pull requests notify | Extends the tray notice | M | 3 |
| P5 | Background work started by the agent, kept after the reply (`Ctrl+B`, `/background`) | Claude Code | Helpers stop with the reply | Owner's call (process cost) | M | 3 |
| P6 | Work in a folder on another machine over SSH | Claude app | This PC only | The CLI runs there, Faber drives it | L | 3 |

## Extensions and memory

| # | Gap | Who has it | Faber today | Way | Size | Wave |
|---|---|---|---|---|---|---|
| X1 | See and switch MCP servers and connectors, with their status (`/mcp`, apps) | all four | Project MCP server list and catalog switches; connectors later | The catalog, MCP part | M | 2 |
| X2 | See, add and remove skills and plugins (`/skills`, `/plugin`) | both CLIs, both apps | Catalog skills for the agent (R247) | Extends the catalog | M | 2 |
| X5 | Reply style: concise, explanatory, a custom one (`/output-style`, styles, personality) | all four | Done: shared reply style in Settings | Settings, passed through each CLI's persona instructions; Codex also gets length | S | 2 |
| X6 | Hooks: see which run and where they come from (`/hooks`) | both CLIs | One switch | Extends the hooks row | S | 3 |

## Making things

| # | Gap | Who has it | Faber today | Way | Size | Wave |
|---|---|---|---|---|---|---|
| M1 | Visual answers: a page, chart or app beside the chat, with versions (artifacts, canvas, inline visuals) | all four | Begun: offline HTML/SVG reply blocks and versions; network grants, project files, Mermaid and built-in pieces remain | Extends the running column's pane; task 310 lists the remaining steps | L | 2 |
| M3 | Create and edit images | ChatGPT, Codex | None | Codex's image tool; phase 6 | L | 3 |
| M4 | Documents: Word, slides, sheets, PDF, ready to send | claude.ai, Codex templates | Only what a CLI skill writes | Catalog skills plus a file card | M | 3 |
| M5 | Comments on a selected part of an answer for the AI to act on | ChatGPT canvas | None | With M1 | M | 3 |
| M6 | The agent sees and uses the PC's other apps, with the person's approval per app (computer use) | Claude app, Codex app | Faber's browser only | Phase 9, with the browser's turn rules | L | 3 |

## Away from the PC and over time

| # | Gap | Who has it | Faber today | Way | Size | Wave |
|---|---|---|---|---|---|---|
| R1 | Scheduled and repeating tasks, one-off reminders (`/loop`, `/schedule`, ChatGPT tasks) | all four | Keep going (until done) | Phase 7, local only | M | 3 |
| R2 | Drive a session from the phone (Remote Control) | Claude Code, Codex | None | Phase 7 | L | 3 |
| R3 | A session that keeps working while the window is closed | Claude Code agents, Codex cloud | Faber keeps running in the tray | Done by the tray; the cloud part is a non-goal | — | — |

## Comfort

| # | Gap | Who has it | Faber today | Way | Size | Wave |
|---|---|---|---|---|---|---|
| K1 | The list of shortcuts (`?`), and keys for model, effort and mode | both CLIs, both apps | Done: ? lists every key, read from the one list of keys | A shortcut sheet; keys stay fixed | S | 2 |
| K2 | Search in Settings | Codex app | Five short sections | Only if Settings grows | S | 3 |
| K3 | A health check that finds and fixes setup problems (`/doctor`) | both CLIs | The account cards check | Extends the account cards | S | 3 |
| K4 | What's new in the CLI itself (`/release-notes`) | Claude Code | Faber's own notes | Owner's call | S | 3 |
| K5 | Sites Faber's browser may or may never open | Claude app | Any http or https page | A Settings list | S | 3 |

## Not to be closed

Each with its reason; a row moves back up only with the owner's word.

- Team, admin, managed policies and sharing with an organization: a non-goal of [README.md](README.md#non-goals).
- Public share links: they need a server that holds the conversation; Faber exports a file instead (F4).
- Cloud sessions, `/teleport`, `/ultrareview`, cloud routines and cloud tasks: they run on the vendor's servers and are
  billed apart; Faber works on this PC with the subscriptions.
- Installing the Slack or GitHub apps, upgrade and credit pages, the mobile app download: the vendor's own accounts.
- Custom GPTs: the same need is met by skills and subagents from the catalog (X2).
- ChatGPT's connectors and driving a chat website: a non-goal of [README.md](README.md#non-goals).
- Pets, terminal colours and themes for the TUI: Faber has its own looks.

## Keeping the list complete

- **Every CLI update.** When a new Claude Code or Codex version is installed, its new commands and flags (its
  `--help`, its slash list, its changelog) are compared with this page; a new one is a new row or a line under "Not to
  be closed". The nightly CLI version tests of phase 4 make this automatic; until then the weekly coherence pass
  (`testkit/coherence/weekly.md`) does it.
- **Every app update.** A new inventory of the Claude and ChatGPT desktop apps in the corpus adds its new features
  here the same way.
- **The direction check** (`scripts/direction.mjs`, R243) reads this page with the other product pages, so a gap with
  no task becomes a proposed task.

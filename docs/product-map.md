# Product map

What Faber has today, for whoever builds the next part. [product-rules.md](product-rules.md) says where a new thing goes;
this page says what is already there, so a new part extends it instead of adding a second way to do the same thing. How
each part looks and moves is in [design.md](design.md); where Faber is going is in [roadmap.md](roadmap.md).

The map names files and keys, never line numbers. `engine/tests/product_map.rs` fails when a setting, a `ui/*.tsx` file,
a shell command or a browser storage key is missing from it; words, flows and features stay true because the commit
that changes them updates this page too. Last full audit: 2026-10-16.

## Words

| Word | What it means | Where it lives |
| :--- | :--- | :--- |
| Chat | The area for conversations with one AI at a time. A chat works in a private folder of its own, or in a folder picked before its first message. | `ui/Welcome.tsx`, `ui/Thread.tsx`, `ui/Composer.tsx` |
| Projects | The area for git folders. Inside a project the person talks to an AI agent that works in the folder, with the same permission modes as Chat (R160). The mission machinery stays in the engine, with no way to start one on screen. | `ui/App.tsx`, `ui/Projects.tsx`, `engine/src/project/` |
| Mission | One goal in one project, done by roles in a separate copy (a git worktree) and landed on Faber's own `faber/…` branch, never on the person's branch. Today (v0): one task, one Writer attempt, no repair loop, a US$2.00 cap. | `engine/src/project/coordinator.rs`, journal in `<project>/.faber/missions/<id>/` |
| Lead, Writer, Reviewer | The roles of a mission: plans the work; changes the code; reviews the exact result. The Reviewer is always the other vendor. | `engine/src/project/` |
| Checks | The commands a project declares under a "Proof commands" heading in its AGENTS.md or CLAUDE.md (or a detected default), confirmed by the person on a sheet when the first mission starts (R137), and read or edited later in Settings > Projects, where Confirm and Save are offered only with the full list and setup on screen (a confirmed list may fold behind "View commands"). Code and specs call them proofs. | `engine/src/project/mission.rs`, `ui/ProjectSettings.tsx`, `ui/Projects.tsx` |
| Grant | What the person allows in a project: the git mode (Safe, `seguro`: a separate copy only; `equilibrado`, push and draft PR, exists in the engine but is not offered) and self-update trust. Changed in Settings > Projects. | `engine/src/project.rs`, `ui/ProjectSettings.tsx` |
| Run mode | Normal or Autonomous on a mission. Stored; nothing reads it yet. | `ui/Projects.tsx` |
| Auto | "AI: choose for me": a small model sorts each message and a fixed table picks the model and effort, paced by how much of each subscription's quota is left. | `engine/src/route.rs`, `engine/src/engine/auto.rs`, `engine/src/budget.rs` |
| Effort | Low, medium or high for a model the person picked by hand; Auto picks its own. | `ui/Composer.tsx`, `engine/src/router.rs` |
| Step up | "Try again one step up": the same question as a branch one effort up on the same model, or on the next stronger model of the same vendor when that rung is not available, offered when a check failed or code changed unchecked. | `engine/src/nudge.rs` |
| Branch | Editing a past message starts a new conversation from that point. | `ui/Thread.tsx` |
| Queue, steer | A message sent while an answer runs waits in a queue; Codex can take it into the running turn. | `engine/src/engine.rs` |
| Permissions | Five modes for Chat: "Ask", "Accept edits", "Plan", "Auto", "Full access" (`dontAsk`, the default). Auto runs as "Accept edits" when the CLI has no auto mode, and the screen says so. Missions run in an isolated mode nobody picks. | `engine/src/model.rs`, `engine/src/permissions.rs` |
| Permission card, held-back request | A question the CLI asks in Ask mode; a request it denied because nobody could answer, shown and never allowed by itself. | `ui/Ask.tsx`, `engine/src/engine/parked.rs` |
| Needs you | Amber, always with words: something waits for the person. | run button, running column, sidebar, tray |
| Running column | The list on the right of everything running, grouped by project. | `ui/Running.tsx`, `ui/running.ts` |
| Quota window, pace, plan | A subscription's usage limit over a time window; how fast it is being spent against the time left; the plan's name (asked when the CLI cannot tell). "Still counting" marks use the vendor has not counted yet. | `engine/src/ledger.rs`, `engine/src/engine/usage_view.rs` |
| Account cards | The screen that installs a CLI, signs in through the CLI's own login and checks it. | `ui/Setup.tsx`, `ui/setup.ts`, `src-tauri/src/setup.rs` |
| First run, tutorial | The two steps a new person goes through, and the cards that follow on the first chat. | `ui/FirstRun.tsx`, `ui/Tutorial.tsx`, `ui/tour.ts` |
| Look | A theme (Dark Room `sala`, Workshop `oficina`, Workbench `bancada`) or a preset (Morning `manha`, High contrast `alto`, Cobalt `cobalto`), plus accent, background, fonts, text size, spacing, corners, motion and effects. | `src-tauri/src/themes.json`, `src-tauri/src/look.rs`, `ui/Appearance.tsx` |
| Curioso | Faber's mascot, the small mark that peeks into empty states. | `ui/Mark.tsx` (`Peek`) |
| Black hole, spark | The Dark Room scene of characters behind a project's start card, lit by a spark when it opens. | `ui/Hole.tsx` |
| Ambient | The quiet living backdrop of the Chat home and the Projects home. | `ui/Ambient.tsx` |
| Ghost | The blinking caret and rotating hints inside an empty composer. | `ui/Ghost.tsx`, `ui/idle.ts` |
| Recents, environment | The last folders opened, and a named set of folders opened together. | `ui/Recents.tsx`, `engine/src/recents.rs` |
| Local build | Faber building and promoting itself from landed missions (slots A and B, a canary, a last good build). Only when started as the development channel. | `ui/LocalBuild.tsx`, `src-tauri/src/local_update.rs`, `engine/src/project/update.rs` |
| Right-click menu | Faber's own short menu; the browser's menu never shows. | `ui/ContextMenu.tsx`, `ui/contextmenu.ts` |
| Work log | The folded activity under an answer: steps, commands, thinking. | `ui/worklog.ts` |
| Preview, stable | The two update channels (R156): stable is the weekly release everyone gets; a preview is the newest build, a few days early, for people who turn on "Receive previews" in Settings. | `src-tauri/src/update.rs`, `scripts/channel.mjs`, `.github/workflows/` |
| Cache warm | Under the latest answer: until when the vendor's prompt cache keeps the next message cheaper. | `ui/Thread.tsx`, `ui/running.ts` |

## Screens and surfaces

One window, no router: `ui/App.tsx` swaps what shows by state. First run or the account cards cover everything; Settings
replaces the shell; otherwise the shell shows the sidebar, the topbar, the work area and the running column.

| Surface | Shows when | What a person does there | Files |
| :--- | :--- | :--- | :--- |
| Titlebar | Always | Drag; minimize, maximize, close (close hides to the tray) | `ui/WindowControls.tsx`, `ui/Mark.tsx` |
| First run: accounts | `setup` unset and no conversation yet | Pick Claude, Codex or "None for now"; install, sign in (the CLI's own login, in a console), check again; Skip or Continue | `ui/FirstRun.tsx`, `ui/Setup.tsx`, `ui/setup.ts` |
| First run: look | After accounts, or once on an older install with no look | Three looks with a live preview, text size, spacing, motion, "Do you already use AI?", an optional name; Back, Continue | `ui/FirstRun.tsx` |
| Tutorial | First fresh chat with a signed-in AI while `tutorialDone` is false | Three cards for a beginner, two for an experienced person; Next, Skip | `ui/Tutorial.tsx`, `ui/tour.ts` |
| Account cards, later | "Connect a subscription" (no AI), or "Manage accounts" in Settings > Connected accounts or Usage limits | The same cards; Close returns where it came from | `ui/Setup.tsx` |
| Sidebar | The shell | New conversation (Ctrl+N); Chat or Projects; search over titles and project names; add a project folder; project rows; chats grouped by day with × delete; footer with Settings, the name, one quota line per AI and the version | `ui/Sidebar.tsx`, `ui/sidebar.ts` |
| Topbar | The shell | Crumb; read-only model chip (Chat only); run button "{n} running · {m} need you" (Ctrl+.) | `ui/App.tsx` |
| Chat home | Chat area, an AI installed, an empty conversation | Greeting, composer with Ghost hints, three suggestions that fill the composer, "Pick up where you left off" (four, waiting ones first, both areas), Ambient | `ui/Welcome.tsx`, `ui/Composer.tsx`, `ui/Ghost.tsx`, `ui/Ambient.tsx` |
| No AI | No CLI installed | "No AI found", "Connect a subscription" | `ui/App.tsx` (`Empty`) |
| Chat conversation | A chat is open | Messages; work log; permission, held-back and quota cards; file cards (preview, save, show); answer actions (copy, step up, "Redo with…", open in terminal, cost, cache warm); one status line with "Details in Settings"; the composer below | `ui/Thread.tsx`, `ui/Ask.tsx`, `ui/QuotaCard.tsx`, `ui/worklog.ts` |
| Question card | The agent asks the person something (the `AskUserQuestion` tool; Claude Code only, Codex's equivalent is off) | A card inside the agent's message: each question with its options to tap (several when it allows), a line to write another answer, Send answers, Skip. Shown in every permission mode; the answers go back to the agent as its tool result | `ui/Ask.tsx`, `engine/src/claude.rs`, `engine/src/engine/answer.rs` |
| Composer | Chat only | Text, paste or drop files, attach, model, effort (manual model only), Stop, Send; warms the CLI on the first key and lets Auto route the draft | `ui/Composer.tsx`, `ui/models.ts`, `ui/composer-size.ts` |
| Projects: open a project | Projects area, no project selected | "Add project folder", Recents and environments, pick up (three project conversations) | `ui/Projects.tsx`, `ui/Recents.tsx` |
| Projects: new conversation | A project, no conversation selected | The new-conversation screen of Chat (greeting, composer, suggestions, pick-up list) scoped to the project's folder: the chat placeholder names the folder, and the model and permission mode work as in Chat | `ui/App.tsx`, `ui/Welcome.tsx`, `ui/Composer.tsx` |
| Projects: start card (no longer reachable: a selected project opens a conversation) | Was: a project with no mission selected | Greeting, goal box with Start, black hole behind, up to three conversations to pick up; still in the code until the mission screens go | `ui/Projects.tsx`, `ui/projects.ts`, `ui/Hole.tsx` |
| Projects: checks sheet (no longer reachable: no mission starts on screen) | First mission start in a project, only in Ask, Accept edits or Plan (Auto and Full access confirm the declared list by themselves, R159) | A compact panel with the check count and source and every command (and the setup that runs first) listed in full, never collapsed, a long list scrolling inside it; Confirm and start, or Not now. Confirming sends exactly the list and declaration shown, and the engine refuses it if the project changed them meanwhile; the panel then shows the new list. A refused start on changed checks reopens it. Asked once per list | `ui/Projects.tsx` |
| Projects: mission board (no longer reachable: no mission starts on screen) | A mission selected | One column: state, New mission (done, paused or blocked; asks first unless done), goal, step line, why it waits, Stop or Resume; needs-you and decision cards; report when done; tasks grouped by Ready, Running, Review, Done, Blocked, empty groups hidden, one line before the plan; folded "Run mode, cost and build" with run mode, own cost against the cap, account pace and local build | `ui/Projects.tsx`, `ui/LocalBuild.tsx` |
| Past project conversation | Opened from the sidebar or a pick-up list | Read only: no composer | `ui/Thread.tsx` |
| Running column | Run button, Ctrl+., or by itself 1.5 s after typing stops | Running conversations by project with their state in words; permission and held-back cards; open a conversation | `ui/Running.tsx`, `ui/running.ts` |
| Settings | Ctrl+,, the sidebar button, a quota line (opens Usage limits), "Details in Settings" | Six sections (below); Back or Esc | `ui/Settings.tsx`, `ui/ProjectSettings.tsx`, `ui/Appearance.tsx`, `ui/draft.ts` |
| Right-click menus | Right-click, the Menu key or Shift+F10 | Per target, below | `ui/ContextMenu.tsx`, `ui/contextmenu.ts` |
| Save-error toast | A save fails | Read and dismiss | `ui/App.tsx` |
| Update opening screen | A window that opens while Faber looks for, downloads or installs an update (R157) | The mark, "Looking for an update…", then "Downloading update… 42%" (megabytes when the size is unknown) with a thin bar, then "Installing…"; nothing to press, the window's buttons stay in reach. The sidebar shows the same words as a quiet line when the window opens in the middle of a download or an install | `ui/UpdateStage.tsx` |
| What's new | First start after an update | One calm page of the release's notes; Continue or Esc closes it | `ui/WhatsNew.tsx`, `ui/notes.ts` |
| Tray | Always, outside the window | Open or Quit; a number while something needs the person; one notification per question, mission decision or held-back request, unless notifications are off | `src-tauri/src/tray.rs`, `src-tauri/src/tray/` |
| Outside windows | The person's own action | Native pickers (project folder, chat folder, theme file, save a file); the file manager ("Open folder", "Show"); a visible console ("Open in terminal", "Sign in") | `src-tauri/src/main.rs`, `src-tauri/src/terminal.rs` |

Shared parts to reuse before writing a new one: `Segmented` and `Toggle` in `ui/controls.tsx`, `Icon` and `VendorMark` in
`ui/Glyphs.tsx`, `Mark`, `Lockup` and `Peek` in `ui/Mark.tsx`, the menu in `ui/contextmenu.ts`, idle motion gates in
`ui/idle.ts`, list and details motion in `ui/motion.ts`, focus traps in `ui/focus.ts`. `ui/main.tsx` is the entry: it
listens for events, applies the look and installs the right-click menu before it draws `App`.

Right-click menus:

| Target | Items |
| :--- | :--- |
| A conversation row | Rename, Delete (confirm) |
| A project row | New conversation in this project (opens its start card), Open folder, Remove from Faber (confirm; refused while it runs) |
| Your message | Copy, Edit |
| An answer | Copy, Redo (as a branch, with the composer's current choice) |
| Selected text | Copy |
| A text field | Cut, Copy, Paste, Select all |
| A recent folder or environment | Remove (a second menu inside `ui/Recents.tsx`, see Known incoherences) |

Keys: Ctrl+N new conversation (in Projects: the current project's start card), Ctrl+, Settings, Ctrl+. running column,
Esc closes the top layer (menu, sidebar overlay, running column, account cards, Settings), Enter sends and Shift+Enter
adds a line. Browser keys (F5, F7, Ctrl+R, P, S, U) do nothing.

## First run

| Step | What the person does | What is saved |
| :--- | :--- | :--- |
| Window opens | Nothing: the system language and Dark Room | Nothing |
| Accounts | Picks the subscriptions, installs and signs in to the CLIs | `setup`: `done` on Continue, `skipped` on Skip (both go on the same way) |
| Look | Look, text size, spacing, motion, "Do you already use AI?", an optional name | One save: `appearance`, `experience`, `userName`, and `tutorialDone: false` unless it is already true |
| First chat | Lands on the Chat home | Nothing |
| Tutorial | Only once an AI is signed in | `tutorialDone: true` on finish or skip |

Left at their defaults, changeable later in Settings: language, permissions, web search, hooks, notifications, keep awake, autostart.
A project's checks are confirmed on a sheet when its first mission starts (in Auto and Full access the start confirms them by itself), not in the first run; its grant lives in Settings > Projects.

## Settings screen

Six sections, in this order. Nothing else is a Settings section.

- **General**: Language (follow the system, or one of five); Your name; Your CLI hooks (only acts in the guarded modes, not "Full access");
  Permissions for Chat and Projects (five modes, "Full access" by default), Web search; reasoning summary and answer length (Codex only); Chat working folder (with Recents;
  acts on the open chat while it has no message; its controls only appear when Settings was opened from Chat); Notifications
  (Notify me, Play a sound); Updates (Receive previews, off by default, R156); System, Windows only (keep the computer awake
  while Faber works, start with Windows).
- **Projects**: per project, closed until opened: checks (confirm, edit, view), "If a tool is missing" (continue without),
  git mode, self-update trust (switch); folder, base and author under More. Applies to that project only.
- **Appearance**: a draft with a live preview, applied only by "Use this look": six looks, accent (five swatches, any
  colour, hex), background, text font, heading font, text size, spacing, corners, motion, the fixed-meaning colours
  (read only), save or open a theme file; in the preview, contrast readings and three effect switches; Back to default.
- **Connected accounts**: each AI's state; "Manage accounts" opens the account cards.
- **Usage limits**: per connected AI, the plan (a question when the CLI cannot tell), the state in words, Faber's own
  spend in API-equivalent dollars, notes, and each window's use, pace marker, projection, reset time, last reading and
  share not counted yet.
- **Privacy**: three statements (no telemetry, history stays on this PC, CLI logins are never read). No controls.

Choices that live elsewhere on purpose: model and effort (the composer, per conversation); paying with credits, moving to
the other subscription or waiting (the quota card, just in time, per message); permission answers (the cards).

## Every setting

`settings.json` in the data folder holds the struct `Settings` of `engine/src/model.rs`; `ui/api.ts` mirrors it by hand.
The only writer is the `save_settings` command; the merge in `Core::save_settings` (`engine/src/engine.rs`) builds a full
`Settings`, so a new field does not compile until it is merged. A patch sets a value and never clears one (an empty
`userName` is the only way back to none). There is no "settings changed" event: Rust readers read the file when they need
it, and the screen takes the saved settings from the answer of `save_settings`.

"Lives on" is one of a closed list: a Settings section, the first run, the composer, a just-in-time card, or none. A
choice a person makes anywhere else (a work screen, a banner) is an incoherence.

| Key | Values (default) | Lives on | Read by |
| :--- | :--- | :--- | :--- |
| `appearance` | A look: `base`, `accent`, `ground`, `textFont`, `headingFont`, `textSize`, `density`, `corners`, `motion`, `effects`, `colors` (Dark Room) | Settings > Appearance; the first run (`base`, `textSize`, `density`, `motion`); `colors` only arrives in a theme file opened there | `src-tauri/src/look.rs`, then the screen |
| `language` | `""` follows the system, `en`, `pt-BR`, `es`, `fr`, `de` (none: the system) | Settings > General | `ui/i18n.ts`; the tray through `set_shell_texts` |
| `userName` | Up to 40 characters (none) | Settings > General; the first run | Greeting, sidebar footer, and both CLIs (copied into each new conversation) |
| `permissions` | `ask`, `acceptEdits`, `plan`, `auto`, `dontAsk` (`dontAsk`) | Settings > General; the composer chip in Chat | The engine at every turn; the status line in a conversation |
| `webSearch` | `off`, `cached` (Codex only), `live` (`live`) | Settings > General | The engine at every turn, for Chat and the person's own project sessions |
| `reasoningSummary` | `default`, `auto`, `concise`, `detailed`, `none` (`default`: the model's own) | Settings > General (Codex only) | The engine at every Codex turn |
| `verbosity` | `default`, `low`, `medium`, `high` (`default`: the model's own) | Settings > General (Codex only) | The engine at every Codex turn |
| `grantsEpoch` | A random token (none). Internal state: a person never sets it | None; only the engine writes it, when a stricter mode replaces the old one | The engine, to drop an "Always allow" made under a looser mode |
| `userHooks` | true, false (false) | Settings > General | The engine; acts only in the guarded modes (not "Full access") |
| `notifications` | true, false (true) | Settings > General | The tray |
| `notificationSound` | true, false (true) | Settings > General | The tray |
| `previews` | true, false (false) | Settings > General (Updates, "Receive previews") | `src-tauri/src/update.rs`, at every update check: on reads the preview manifest, off the stable one; a version that is not newer is never installed |
| `keepAwake` | true, false (true) | Settings > General (System) | `src-tauri/src/awake.rs` |
| `autostart` | true, false (false) | Settings > General (System) | `src-tauri/src/autostart.rs`, which also writes the Windows Run key |
| `setup` | `done`, `skipped` (none: the first run is due) | The first run (the account cards' Close also writes it) | `ui/setup.ts` only |
| `experience` | `beginner`, `experienced` (none counts as beginner) | The first run only | The tutorial |
| `tutorialDone` | true, false (none before the first run) | The first run and the tutorial's end | The tutorial |

## Other stored state

The data folder is `%APPDATA%\Faber` (or `FABER_DATA_DIR`). JSON files are written through a temporary file and renamed.

| Where | What | Written by |
| :--- | :--- | :--- |
| `conversations/<id>.json` | One conversation: messages, `choice` (model and effort), `workingFolder`, `allowAlways` (tools always allowed in it), the copied `userName`, CLI sessions, queued messages, a waiting quota recovery | The engine |
| `projects.json` | Projects: id, name, path, git identity (`repo`), `grant`, the confirmed checks (`extra.proof`) | Add, open or remove a project; Settings > Projects; the checks sheet |
| `<project>/.faber/missions/<id>/` and `missions/<id>/` | A mission's journal (mission, plan, contracts, decisions, artifacts, report) and its run, runtime and effects records; in `missions/<id>/proofs/`, each check's whole output (`<receipt>.output`) and the short report the roles read (`<receipt>.report`, R155), with every line that looks like a secret withheld | The mission engine |
| `workspaces/<project>/` | The persistent working copy where a project's setup and checks run | The mission engine |
| `qualification/` | Receipts of the hidden check of the installed CLIs before missions (valid 24 h) | The shell, before a mission starts |
| `recents.json` | Recent folders and environments | Opening folders, Recents |
| `budget.json` | The quota ledger (readings, learned weekly shape, own spend, typed plan) and the mission budget; a damaged copy is set aside as `budget.json.quarantine` | The engine |
| The development root | Slots, manifest, canary and rollback records of the local build | `src-tauri/src/local_update.rs` |
| `seen_decisions.json` | Mission notices already shown, so a restart does not repeat them | The tray |
| `notes-seen` | The last "what's new" entry the person has seen, so it shows once per update | `save_notes_seen` in the shell |
| `cache-health.jsonl` | A local log of cache misses, never sent anywhere | The engine |
| `chat/<id>/`, `attachments/<id>/` | A chat's private folder; a project conversation's attachments | The engine |
| Browser storage `faber.place` | The last area, project and conversation | `ui/App.tsx` |
| Browser storage `faber.choice.<conversationId>` | The model and effort picked for a conversation (the conversation file keeps a copy) | `ui/App.tsx` |
| Browser storage `faber.project.goal.<projectId>` | The goal being typed on a start card | `ui/projects.ts` |
| Browser storage `faber.project.mission.<projectId>` | The mission shown on a project's board; New mission clears it | `ui/projects.ts` |
| Browser storage `faber.unsent.<field>` | Text typed and not sent, so an update restart or a crash loses nothing | `ui/unsent.ts` |
| Outside Faber's folders | The Windows Run key (autostart); CLIs installed in `~/.local/bin` and added to the user PATH | `src-tauri/src/autostart.rs`, `engine/src/install.rs` |

Environment variables are for tests and development: `FABER_DATA_DIR` (also turns off single instance, the updater and
the real keep-awake and autostart), `FABER_HIDDEN`, `FABER_CHANNEL`, `FABER_CDP_PORT`, `FABER_CHECKS_OUT`,
`FABER_CLI_CLAUDE`, `FABER_CLI_CODEX`, `FABER_SETUP_SOURCE`. Nothing a person chooses lives in one.

## Shell commands

The fixed list the screen may call (`generate_handler!` in `src-tauri/src/main.rs`; `setup_*` live in
`src-tauri/src/setup.rs`), typed in `ui/api.ts`. Commands return error keys (`error.*`, `reason.*`), never
sentences; `open_link`'s English error and the raw system errors some commands pass on are the exceptions (incoherence
23). Test builds add `report_checks` and two mission probes.

| Group | Commands |
| :--- | :--- |
| Conversations | `state`, `open`, `send`, `branch`, `warm`, `route_draft`, `interrupt`, `answer`, `answer_question`, `rename`, `remove`, `remove_queued`, `send_queued`, `chat_folder`, `open_terminal` |
| Quota | `set_clock`, `quota_attention`, `answer_plan`, `approve_credits`, `move_turn`, `wait_turn` |
| Files and links | `save_file`, `show_file`, `read_image`, `open_link` |
| Settings and looks | `save_settings`, `preview_look`, `save_theme`, `open_theme`, `set_shell_texts` |
| Projects and folders | `add_project`, `remove_project`, `show_project`, `recents`, `open_folders`, `forget_recent`, `save_environment`, `forget_environment`, `start_environment` |
| Missions | `project_enroll`, `project_grant`, `project_revoke` (no caller on screen yet), `project_proof`, `project_proof_confirm`, `mission_start`, `mission_state`, `mission_mode`, `mission_stop`, `mission_resume`, `mission_answer`, `local_update_state` |
| Updates | `update_status` (phase and progress), `screen_ready` (the first frame is painted: the window, built hidden, shows; a failsafe shows it after 3 s; the page paints its own mark on the theme colour before any script, so neither shows a blank window), `update_restart` (the "Update ready, restart" button), `input_seen` (first typing, paste or drop), `draft_dirty` (a field holds an unsent draft), `notes_seen`, `save_notes_seen` |
| Accounts | `setup_check`, `setup_install`, `setup_sign_in` |

Events: `faber:event` carries `message`, `queue`, `quota`, `usage`, `running`, `list` and the account cards' `setup`
progress; `mission` carries a mission's view after every step; `agents` carries the check of the CLIs before a first
mission.

## Features and their checks

A change extends the check of its feature; a new feature adds one. Engine tests are listed in `engine/tests/all.rs`, screen
checks in `tests/ui/index.ts`, real-app scenarios in `testkit/src/screen/mod.rs`.

| Feature | State | Checks |
| :--- | :--- | :--- |
| Chat with Claude Code and Codex: streaming, tools, images, attachments, files | Done | `engine/tests/claude_session.rs`, `codex_session.rs`, `session_resume.rs`, `workspace.rs`; `tests/fixtures/{claude,codex}/`; `tests/ui/thread.check.ts` |
| Model and effort | Done | `tests/ui/models.check.ts`, `effort.check.ts` |
| Auto and step up | Done; the draft is routed at every pause (the roadmap wants at most twice) | `engine/tests/auto.rs`, `route.rs`, `simulation.rs`; `tests/ui/nudge.check.ts` |
| Quota reading, pacing, recovery, plan question | Done | `engine/tests/pacing.rs`, `quota.rs`, `quota_live.rs`, `budget.rs`, `ledger.rs`, `learn.rs`, `paid_guard.rs`; `tests/ui/pace.check.ts`, `quota.check.ts` |
| Queue, steer, branch | Done | `engine/tests/engine.rs`; `tests/ui/steer.check.ts` |
| Permissions, cards, held-back requests | Done (five modes, R140) | `testkit/src/screen/permissions.rs`; `engine/tests/project_security.rs`; `tests/ui/thread.check.ts` |
| Running column, tray, notifications | Done | `tests/ui/running.check.ts`; tests inside `src-tauri/src/tray/decide.rs` |
| Keep awake, start with Windows | Done on Windows | tests inside `src-tauri/src/awake.rs`, `autostart.rs` |
| Public self-update | Done, on two channels (R156): stable reads the latest release's `latest.json` and previews (opt-in) the `preview-channel` release's, which every release replaces with its newest build; checks at start and every 6 h; installs on quit or, on Windows, at the first moment no conversation or CLI install runs and the window is hidden (a running mission does not hold it back, incoherence 29) (R141: it also installs at start, before the first input and with no unsent draft; "What's new" shows once after) | tests inside `src-tauri/src/update.rs` |
| First run, account cards, tutorial | Done | `tests/ui/setup.check.ts`, `tutorial.check.ts`; `testkit/src/screen/setup.rs`, `first_run.rs` |
| Settings, looks, theme files | Done | `tests/ui/settings.check.ts`, `appearance.check.ts`; `testkit/src/screen/settings.rs` |
| Five languages | Done; part of de, es, fr and pt-BR still awaits a native speaker's review | `tests/ui/i18n.check.ts`, `mojibake.check.ts` |
| Right-click menus, rename, remove a project | Done | `tests/ui/contextmenu.check.ts`; `testkit/src/screen/menus.rs` |
| Recents and environments | Done | `engine/tests/recents.rs`; `tests/ui/recents.check.ts` |
| Homes and idle motion | Done | `tests/ui/idle.check.ts`, `hole.check.ts`, `motion.check.ts`; `testkit/src/screen/homes.rs` |
| Missions | v0 (see Words) | `engine/tests/project_*.rs`; `tests/fixtures/projects/`; `tests/ui/projects.check.ts`; `testkit/src/screen/coordinator.rs`, `projects.rs`, `projects_ui.rs` |
| Local build (development channel) | Done, developer only | `engine/tests/local_update.rs`, `src-tauri/tests/local_update.rs`, `testkit/src/screen/local_update.rs` |
| UX sentinel and memory budget | Done | `testkit/src/sentinel.rs`, `testkit/src/screen/app.rs` (400 MB) |
| This map | Done | `engine/tests/product_map.rs`; the trap tasks and the weekly coherence pass in `testkit/coherence/` (R151) |

Not built yet, so nothing may assume it: the catalog and live manifest, the toolbox, Livre, snapshots and undo, visual
answers, the benchmark, asking Faber about Faber, full-text search, moving a chat to a project, the what's-new screen
(R142), installing updates when Faber opens (R141), Equilibrado on screen, decisions
produced by a mission, missions with more than one task.

## Known incoherences

Found by the 2026-10-16 audit and not fixed yet. Touching one of these parts means fixing it or leaving it as it is,
never copying it. "Owner" marks a choice of taste or direction that needs the owner first. Delete a line in the commit
that fixes it. The weekly coherence pass (`testkit/coherence/weekly.md`) proposes new lines and the removal of fixed
ones.

1. Settings on a work screen (C011): the start card is calm now (checks, tools, git mode, self-update trust and Details
   live in Settings > Projects); the board still carries the run mode, folded under "Run mode, cost and build".
2. Mission needs are invisible inside the window: the tray counts mission decisions and blocked missions, but the run
   button, the running column, the sidebar and the pick-up lists only count conversations.
3. A left mission cannot be opened again: no list of a project's missions exists, so New mission asks first on a paused
   or blocked one.
4. "If a tool is missing" is never sent to the engine; "Find and install it for me" only shows "not ready yet".
5. `project_revoke` has no caller: a grant (and self-update trust) can never be withdrawn on screen, and Start grants
   again every time.
6. (fixed by R160: a project opens a chat, not a mission start card.)
7. Chat and Projects differ for the same things: pick-up lists (four of all areas against three of the project), the
   goal box is its own textarea instead of `ui/Composer.tsx`, no model chip in Projects, no "No AI" state in Projects.
8. Model choice in four places: the composer, "Redo with…" and step up under every answer, and the answer's
   right-click Redo (which silently uses the composer's choice). design.md gives the choice to the composer
   only; whether the answer's pickers stay is the owner's call (Owner).
9. Two "Open a project" screens (`ui/Projects.tsx` and the leftover `Empty` branch in `ui/App.tsx`) with different
   labels; three labels for picking a folder ("Choose folder", "Choose a folder", "Add project folder").
10. Two right-click menu implementations: `ui/Recents.tsx` has its own, with no confirm on Remove (C009).
11. Three ways to confirm a delete: the × in the sidebar, the menu's Delete, and Recents' Remove with none.
12. Money and quota in three formats and four vocabularies ("Own cost", "Cost so far", "Faber today … API-equivalent",
    "… at API prices"; "Account pace", "Usage limits"), not localized.
13. "Update itself" means three things: the silent updater, the project's self-update trust (Settings > Projects) and the Local build card
    (shown on every mission board even outside the development channel).
14. The same option in two places with different words: the name ("How should Faber call you?" in the first run, "Your
    name" in Settings) and text size (three buttons in the first run saving 13, the theme's own or 16; a slider up to 18
    in Settings).
15. First-run answers that cannot change later: "Do you already use AI?" has no control and the tutorial cannot be
    replayed, yet the first run's footer says everything can be changed in Settings > Appearance.
16. The account cards have three entry labels ("Connect a subscription", "Manage accounts" twice) and two names for one
    account ("Codex" on the card, "ChatGPT" in the no-AI text); Skip and Continue save different values nothing reads
    differently; Close writes `done` even after a Skip.
17. The chat working folder sits in global Settings but changes only the open, still empty chat; opened from Projects,
    the row keeps its label and explanation with no control under them.
18. The hooks switch is always enabled but acts only in the guarded modes (all but "Full access"); Settings' "Permissions for Chat and
    Projects" reads as if it governed missions, which run isolated.
19. Settings has no "Learn more" pattern: General shows paragraphs of three to five sentences (product-rules: one short
    line per option).
20. The model and effort choice is stored twice (browser storage wins over the conversation file); autostart is stored
    twice (the setting and the Windows Run key, which the screen never reads back).
21. Defaults are repeated in each reader (`permissions`, `keepAwake`, `notifications`, `userHooks`, effort `medium`):
    a new setting keeps its default in one place.
22. The tray stays in English when Faber starts hidden (autostart): its words arrive only from an open window.
23. Text outside `ui/locales/`: the `Ctrl N` and `Esc` key labels (also on macOS), `Aa`, `×`, `v{version}`, file size
    units, mission durations, raw vendor ids and raw JSON on task cards, `$` amounts, and "Attached file" injected into
    prompts; in Rust the idle tray tooltip, a dialog filter and `open_link`'s English error.
24. Unused locale keys: `answer.thinking`, `set.effects`, `usage.pace`, `projects.title`,
    `projects.empty.selectProject`, `projects.receipt.verified`, `projects.gitMode.equilibrado`.
25. Every Start grants again and bumps the grant revision, which makes earlier missions stale; `docs/design.md` says the
    grant is given once.
26. Removing a project leaves its missions, working copy and browser keys behind; adding the folder again gives it a new
    id, and its old missions fail with `error.journal.mismatch`.
27. Adding a project reads `projects.json` without the checked reader, so a damaged file reads as empty and the next
    write loses every grant and confirmed check.
28. A project's tail in the sidebar shows the amber bubble with a bare number; design.md says amber always comes with
    words.
29. An update can install while a mission runs: `install_when_idle` (`src-tauri/src/update.rs`) holds back for
    `Engine::is_working` and CLI installs, and `is_working` counts only the conversations of the running list, so mission
    roles and proofs do not hold it back. The mission then waits, paused, after the restart.

## Worked example: adding a setting

Say people want to turn off the cache-warm note.

1. Check the map first: no setting covers it, and it is a preference for every conversation, so it is a field of
   `Settings`, not of a conversation, a project or browser storage.
2. Add `cache_note: Option<bool>` to `Settings` in `engine/src/model.rs`. The merge in `Core::save_settings` and
   `engine/tests/product_map.rs` stop compiling until the field is in both.
3. Give it one default in one place and read it there.
4. Mirror it in `Settings` in `ui/api.ts`.
5. Place its control with [product-rules.md](product-rules.md): a `Toggle` from `ui/controls.tsx` in the Settings section
   it belongs to (General), one short line, never on the conversation screen.
6. Write its label and hint in all five `ui/locales/` files by hand.
7. Extend `tests/ui/settings.check.ts` (and `testkit/src/screen/settings.rs` when it changes the real app).
8. Add its row to "Every setting" above, in the same commit.

What not to do: a second control for something that already has one, like the "Redo with…" select under every answer
next to the composer's model picker (Known incoherences, 8). That is the mistake this page exists to prevent.

# Named application control

## The `faber` tool first (R585)

An agent operates Faber through its own command table before it reaches for the screen. The `faber` tool, offered in Chat
and Projects wherever the front end gave the engine its table (`Engine::set_commands`; Claude Code and Codex in both areas,
Antigravity in Projects, where its tool relay already runs), has two actions. `find` takes words (at most 200 characters
and 16 words) and answers up to 8 commands, each with its description, kind, reach and input schema; `by` is left out of
the schema because the host sets it. `run` takes a command and its arguments and goes through
`Host::dispatch_as(Caller::Agent { conversation })`, the same check the command line and the hidden renderer pass: `own`
acts in the agent's family, `ask` records a request the person answers and returns `error.reach.ask` with its id, and
`person` and `screen` are refused.

## Reach follows the permission mode

In Full access some `ask` commands from the conversation's own agent run at once (the owner's answer 6 of 2026-11-25,
R587). `Host::dispatch_as` reads the mode at the call from the engine, never from the agent's arguments, and only for a
conversation that exists: `reach::full_access` needs the stored mode to be Full access and a session to run in the
conversation in Full access (`Engine::live_mode`). No session (an agent calls only while its turn runs), or one in Plan or
Isolated, keeps the card. `person` and `screen` never follow the mode, nor do the `PERSON_SETTINGS` of `save_settings`, an
`own` command aimed at another family's conversation, or an agent with no conversation.

`reach::in_full_access` lists what runs, and anything it does not list keeps the card, so a new `ask` command asks until
someone decides. Commands that name no conversation and touch nothing another conversation reads run (`show_panel`,
`show_project`, and `save_settings` outside the fields below). Commands that name one run only when it is in the caller's
own family (`move_to_project`, `show_file`, `show_conversation` by `id`,
`rewind_restore` and `rewind_redo` by `request.conversationId`, `stop_background` through the conversation its command
belongs to); another family's conversation keeps the card. These keep the card in every mode, each for its reason:

- deleting for good, which no undo brings back: `remove_chat_group` and `remove_chat_group_file` (stored attachments),
  `forget_auto_learned`, `forget_environment`, `remove_project`, `project_goal_remove`, `forget_recent`;
- spending the person's quota, which stays with the person: `new_group_chat`, `pick_up` and `wake_wait` start a turn,
  `project_goal_resume` restarts fixing that starts turns, `project_goal_add` and `project_goal_add_crashes` add a goal
  whose checks and fix turns start on their own in a project with automatic goals, and `save_settings` touching
  `CARD_SETTINGS` (`webSearch`, `defaultModel`, `reasoningSummary`, `strongerWhenStuck`, `resumeInterrupted`), the reply
  style (`replyStyle`, `replyStyleText`, the AI's own instructions in every conversation) or `userName` (in every new
  conversation's instructions);
- what other conversations read as their own: a chat group's instructions and files (`save_chat_group`,
  `add_chat_group_files`), which every conversation of the group carries, and an environment's folders
  (`save_environment`), which the next opening of that preset uses;
- `rewind_recover`, which finishes every conversation's interrupted rewind, not only the one it names;
- `update_restart`, which stops running work and launches the installer;
- `open_link`, which sends the person's own browser to an address the agent chose, outside the blocked sites;
- `stop_aside`, whose id does not say whose quick question it stops;
- a tool's install, replacement, removal, switch or restore (`tools_*`) and `setup_install`: R584 and R33 leave an item
  Faber has not checked to one yes, and a plugin can bring hooks; the agent's own `catalog` tool still installs a checked
  item without a card.

A command that runs this way leaves a Faber line (`FaberAct::ran_unasked`, kind `reach`) on the reply under way, else the
conversation's last reply, naming the request card's group in the screen's words (`act.reach.*`, "because this
conversation is in Full access") with Change, which opens Settings. A command that fails leaves "tried to ... and it
failed" (`act.reach.failed.*`), since it may have changed part of what it touched. A conversation with no reply yet gets
no line. Every other mode keeps the card. The command line and the hidden renderer reach the same `dispatch_as`.

What this does not stop: a mode switched while a call is under way (the call was allowed when it started), a command that
ends after the tool stopped waiting for it (120 seconds), and a process of the same conversation using the identity it
inherited (it is that conversation).

`show_panel` lets the AI show the person something: it opens a Settings section (`general`, `projects`, `appearance`,
`accounts`, `privacy`) or the Catalog on the main screen through a `showPanel` event (the hidden renderer drops it). Opening
one Catalog item is left out: it needs screen plumbing the Catalog does not have yet. The check costs a few engine reads
under its lock per ask refusal, no process and nothing kept.

`find` and `run` share one rule (`agent_commands::eligible`), so `find` never lists what `run` would refuse: never a
`person` or `screen` command, never `send` (`error.session.useSession`); in Plan and Isolated, which change nothing, only
true reads (`error.reach.readOnly`), which leaves out the reads in name only that run a program, prepare files or open
something on the person's screen (`setup_check`, `tools_list`, `tools_prepare_replacement`, `project_memory`, `show_file`,
`show_project`, `cli_sessions`); and in a Chat, never a project's own commands (the `project_` ones, the session controls
and environments; `error.reach.projectOnly`). From an agent, `background_commands` lists only its family's commands, as
the hidden renderer's bootstrap does.

The engine finds, runs and prepares the answer away from its lock and takes the lock only to hand the answer over. A run
stops being waited for after 120 seconds; work a command already handed to a blocking thread (a file read, a git call)
cannot be cancelled and runs to its end. An answer past 20,000 characters comes back as valid JSON,
`{"truncated": true, "characters": N, "text": ...}`, cut before any data address it would split.

The owner's line (answer 6 of 2026-11-25): the AI operates Faber in everything except signing in, spending money and
changing its own permissions, trust and hooks. Those are `person` reach: `setup_sign_in` and the `cookie_*` commands;
money: `approve_credits`, `answer_plan`, and every turn started outside the conversation's own (`branch`, `branch_here`,
`follow_up`, `ask_aside`, `edit_queued`, `send_queued`), so no agent escapes the project cap or `startedBy` through a new
root; `answer`, `approve_plan`, `answer_question`, `answer_ask`, `chat_folder`, `open_folders`, `add_project`,
`repoint_project`, `set_mcp_server`, `project_proof_confirm`, `run_app_*` and `edit_project_memory` (a kept decision can
be a "never touch" rule); a project's own switches `start_environment`, `project_copies` and `project_goals_auto`; and
`save_settings` touching `permissions`, `beforePlan`, `permissionsRevision`, `grantsEpoch`, `blockedHosts`,
`extensionsDeveloperMode`, `faberTrust` or `userHooks` (`PERSON_SETTINGS`). Installing, removing, enabling or replacing a
tool stays `ask`. A temporary chat is closed only by a send from the screen, never by an agent's. The tool costs one
definition of under 1,000 bytes per turn, no process and no memory kept between calls.

The hidden renderer below stays for what the table does not cover and for checking what the screen shows.

## The hidden renderer

`browse` accepts `app: "faber-live"` or `app: "faber-test"` explicitly. This is a conversation-owned hidden renderer of the actual shipped UI, supported by Electron and Windows Tauri and connected to that instance's host, not a desktop controller or the person's main renderer. Test targets require a test instance with its own data directory; this tool does not create test data isolation or launch another installed app. Other developed web apps still use their local `url` in the browser pane. Unsupported native applications need a separate supported adapter and are not claimed here.

Actions: `read` (default), `click`, `type`, `scroll`, `screenshot`, `close`. Read returns visible text and numeric refs for visible interactive elements. Click/type require a ref from that session's last read; disconnected, relabelled, obscured or offscreen elements are refused. Type validates an enabled editable field before any click and inserts text through Chromium. Text is capped at 2,000 characters. `up` reverses scroll. Optional `size: "720x900"` resizes only this hidden screen (300..2560 pixels per side; Faber supports desktop layouts from 720 pixels). Screenshots are fresh JPEG attachments in the existing browser reply. App content is untrusted data.

Every IPC call from this renderer, even a delayed effect after input returns, is sent through `Host::dispatch_as(Caller::Agent { conversation })`. No timing-based Screen privilege swap exists. The only bootstrap accommodations are read-only app state and family-scoped conversation reads; `screen_ready` and `set_clock` are shell no-ops. Main-screen window controls, folder dialogs, sign-in, permission approvals, money, and other Person/Screen operations remain refused. Own and Ask rules remain checked by the host. Password and one-time-code fields cannot be selected or typed. No arbitrary JavaScript, protocol command, URL, desktop input or remote debugging port is exposed to the agent.

The hidden renderer uses the instance's backend: live operations may change real data when agent policy permits. It is not an isolated copy of live data. One conversation renderer at a time, closed when its owning reply ends or stops, after 60 seconds without a call, explicitly on `close`, and at shutdown. No renderer exists before first use. Measured with the real first-run UI and a temporary host: Chromium working set rose from 62,488 KiB to 340,916 KiB (278,428 KiB increase). This excludes the Rust sidecar and is a one-shot working-set sample, not an idle-memory or CPU qualification. Offscreen drawing is capped at 10 frames per second. Existing browse restrictions, turn protections and installed-app debugging refusal remain in place. The hidden renderer never calls show/focus and does not show an OS window.

Bootstrap state filters background command output and running rows to the authenticated caller's family. Its `by` identity is injected by the host; sequence arguments cannot forge it. Live event forwarding denies unknown and unrelated conversation payloads. Electron admits another conversation's stream only after the host authorizes opening it, and discards those subscriptions with the renderer.


## Tauri on Windows

Rejected stale, disabled or obscured controls retain the renderer and its draft for a new read and retry. Transport failures and timeouts close it. Cancellation during creation reserves the session until the pending action finishes cleanup, so a late completion cannot recreate a closed view. Agent sends retain the host's existing refusal; reading through `app_control_open` never changes the person's selection or closes temporary chats.

The Windows Tauri shell routes named app requests through its actual browser adapter. A permanent registry maps each
private child-webview label to its conversation; every invoke, including asynchronous effects, uses that agent's
authority. Host events are sent to the main webview explicitly; control children receive only their own conversation
content and non-conversation refresh/settings/usage events. Family reads retain host checks, but push updates for
siblings are not forwarded. Main-window authority is unchanged. Only one control renderer may exist, with explicit close, 60-second
idle expiry, cancellation/timeout cleanup and shutdown cleanup. Native WebView2 input and screenshots need no
installed-app debugging port or machine-wide input. The private child shares the main WebView2 environment,
allows only its app origin, rejects new windows/downloads and has event-listening capabilities only.

Targets are backend-local: hidden instances with explicit non-default data accept `faber-test`; normal instances accept
`faber-live`. The tool does not launch a test instance. The existing testkit hidden harness launches isolated builds
with their own data and browser profile without copying sign-ins. Test-only hooks exercise the real parser and browser
adapter and are absent from installed builds. macOS/Linux remain unsupported. Native end-to-end evidence and memory
measurements belong in the Tauri verification report; compilation alone does not establish working UI control.


Tauri frontend event subscriptions explicitly target the current webview, including the main renderer; global
subscriptions would receive targeted emits too. The hidden native canary verifies that unrelated/main-only events
never reach the controlled renderer, own-conversation events and settings do, and child-only messages do not reach main.
The native proof also exercises actual UI at 1280 and 720 pixels, Person-operation refusal and trusted click/type.
Terminal messages (done, stopped or error) for the owning conversation close its renderer immediately; observed
children do not revoke their parent's renderer. Idle timeout remains a fallback. Sustained CPU, timed idle expiry and
forced-abort runtime qualification are not claimed by these receipts.

Electron uses one reusable memory-only control profile, cleared before each owner opens it. Its asset protocol and CSP match the main screen; permissions, devices and downloads are denied. Test identity additionally requires an explicit data path outside the default data tree, resolving existing junctions and Windows case aliases.

Native test identity rejects canonical data paths that equal, contain, or sit below the default data tree, including Windows case aliases and existing junction ancestors. Screen-only conversation-navigation and update notices target the main webview explicitly. A test-only storage canary can retain an incognito sibling on the same environment and compare sequential owners; it is not an installed command or agent capability.

Native refs also discover ARIA options, menu items, tabs, checkboxes, radios and switches. A ref records the element's role and label; either changing after the read invalidates the click. Test-only keyboard observation opens the actual command palette to exercise a native trusted option click.

| `screen` | Only the screen itself (`error.reach.screen` over the channel) | `open`, `state`, `set_clock`, `quota_attention`, `parked_seen`, `warm`, `route_draft`, `seen`, `preview_look`, `set_shell_texts`, `input_seen`, `draft_dirty`, `game_in_front`, `notes_seen`, `save_notes_seen`, `site_place`, `site_round`, `site_size`, `site_park`, `site_open`, `site_step`, `site_go`, `site_tab`, `site_history`, `site_clear`, `screen_ready`, `visual_open`, `visual_page`, `visual_close`, `visual_policy`, `report_checks`, `checks_progress`, `test_site_probe`, `test_host_size`, `test_site_refuse_once` || `person` | Only the person on the screen (`error.reach.person` for everyone else): signing in, answering cards, granting folders, security settings, deleting for good, native pickers | `answer_plan`, `approve_credits`, `answer`, `approve_plan`, `answer_question`, `remove`, `save_theme`, `export_conversation`, `open_theme`, `add_project`, `open_folders`, `chat_folder`, `save_file`, `open_terminal`, `project_proof_confirm`, `setup_sign_in`, `set_mcp_server`, `tools_clear_removed`, `cookie_profiles`, `cookie_sites`, `cookie_import`, `cookie_held`, `cookie_forget`, and the security fields of `save_settings` (`permissions`, `userHooks`, `faberTrust`, `grantsEpoch`, `permissionsRevision`, `beforePlan`, `previews`, `autostart`) || `ask` | Person at a terminal or on screen; an AI gets `error.reach.ask` and asks the person | `save_chat_group`, `add_chat_group_files`, `remove_chat_group_file`, `remove_chat_group`, `forget_auto_learned`, `pick_up`, `move_to_project`, `stop_background`, `rewind_recover`, `rewind_restore`, `rewind_redo`, `stop_aside`, `remove_project`, `save_settings`, `save_environment`, `forget_environment`, `start_environment`, `show_file`, `show_project`, `open_link`, `project_copies`, `project_goals_auto`, `project_goal_add`, `project_goal_add_crashes`, `project_goal_remove`, `project_goal_resume`, `edit_project_memory`, `update_restart`, `setup_install`, `tools_install`, `tools_prepare_replacement`, `tools_replace`, `tools_remove`, `tools_set_enabled`, `tools_restore` (`save_settings` for the security fields below is the person's alone) || `own` | Person anywhere; an AI only in its own family (or to start a new conversation) | `move_turn`, `wait_turn`, `continue_closed`, `interrupt`, `send`, `branch`, `branch_here`, `save_piece_value`, `keep_going`, `remove_queued`, `edit_queued`, `send_queued`, `tell_helper`, `compact_now`, `ask_aside`, `clear_asides`, `unfollow_pr` || `free` | Anyone | `conversations`, `conversation`, `projects`, `quota`, `settings`, `create_chat_group`, `move_chat_to_group`, `new_group_chat`, `search_messages`, `prompt_history`, `cli_sessions`, `versions`, `visual_answers`, `visual_pieces`, `rename`, `archive`, `pin`, `child_words`, `child_transcript`, `mentions`, `commands`, `context_parts`, `background_commands`, `changes`, `change_patch`, `rewind_points`, `rewind_redos`, `rewind_redo_preview`, `rewind_preview`, `asides`, `recents`, `forget_recent`, `read_file_at`, `read_image`, `site_tabs`, `site_picture`, `project_goals`, `project_proof`, `project_memory`, `update_status`, `setup_check`, `tools_list`, `tools_inventory`, `mcp_servers`, `tools_in_use` |# The command line

`faber` (`cli/`, shipped as `bin\faber.exe` beside the app) lets an AI inside a conversation, an AI in an outside
terminal, or a person at a terminal do what a person does on the screen. Every CLI session Faber starts gets `bin` first
on its `PATH` and the full path in `FABER_CLI`.

One command table (`host/src/table.rs`, R459) sits behind the screen, the command line and the AI's tools. A command added
to the table is immediately callable through `faber call <command>` and listed in `faber help --json`.

## Reaching Faber

`faber` talks to the running Faber over a local channel (`channel/`), never a network port:

- **Windows:** the named pipe `\\.\pipe\faber-<16 hex>` (FNV-1a 64 of the lowercased data folder path with backslashes and
  no trailing slash), created with a security descriptor that grants read and write only to the signed-in user's SID and
  with `PIPE_REJECT_REMOTE_CLIENTS`.
- **Unix:** `<data>/channel/faber.sock` inside a `0700` folder, with the peer UID checked against `geteuid()` before any
  line is read.
- **Per-run secret:** before listening, Faber writes 32 random bytes from the OS CSPRNG as 64 hex characters to
  `<data>/channel/secret` (readable only by the signed-in user). A client's first line (`hello`, within 5 s) must carry
  that secret; a missing or wrong one fails with `error.channel.secret` and closes.
- **Conversation tokens:** a token `fa1.<id>.<64 hex>` (HMAC-SHA256 with this run's secret over `fa1.<id>`, checked in
  constant time) names the AI's conversation. `hello` passes it (from `FABER_AGENT_TOKEN`) with `FABER_AGENT_DEPTH`; a
  tampered token or one from a previous run fails with `error.channel.token`. Faber does not put a token in the
  environment of the CLIs it starts yet (slice 3, with the AI's own tools): until then every AI Faber starts is an AI with
  no conversation, and asks the person for anything beyond reading and organizing.
- **Starting Faber when closed:** if nothing answers on the channel, `faber` finds Faber's program (`FABER_APP`, or the app
  folder above its own `bin` folder), starts it hidden in the tray (`--autostart`, no console) and
  waits up to 10 s for the channel. `--no-start` and any process with `FABER_AGENT_DEPTH >= 1` never start Faber and exit
  with code `3`.

## Who may do what

From `hello` and the OS peer process ID, Faber knows who is calling (`Caller` in `host/src/reach.rs`):

1. A valid token from this run's secret names `Agent { conversation: Some(id) }`.
2. Otherwise, a process inside one of Faber's CLI jobs on Windows (`winproc::in_any_cli_job`) or with `agentDepth >= 1` is
   `Agent { conversation: None }` (an AI with no conversation family).
3. Otherwise, the caller is `Person` (a person, or an outside AI, in the user's terminal). The window is `Screen`.

Every command in `host/src/table.rs` names one of five reaches, checked in `Host::dispatch_as`:

| Reach | Who may run it | What uses it |
| :--- | :--- | :--- |
| `free` | Anyone | Every read command (`conversations`, `conversation`, `projects`, `quota`, `settings`, `cli_sessions`, `prompt_history`, `search_messages`, `versions`, `visual_answers`, `visual_pieces`, `context_parts`, `asides`, `child_words`, `child_transcript`, `background_commands`, `mentions`, `changes`, `change_patch`, `commands`, `rewind_points`, `rewind_preview`, `rewind_redos`, `rewind_redo_preview`, `recents`, `project_goals`, `project_memory`, `project_proof`, `update_status`, `notes_seen`, `setup_check`, `tools_in_use`, `tools_list`, `tools_inventory`, `mcp_servers`) plus `archive`, `pin` and `rename` |
| `own` | Person anywhere; an AI only in its own family (or to start a new conversation) | `send`, `interrupt`, `continue_closed`, `keep_going`, `compact_now`, `warm`, `route_draft`, `branch`, `branch_here`, `remove_queued`, `edit_queued`, `send_queued`, `tell_helper`, `ask_aside`, `stop_aside`, ` clear_asides`, `unfollow_pr`, `chat_folder`, `move_turn`, `wait_turn` |
| `ask` | Person at a terminal or on screen; an AI gets `error.reach.ask` and asks the person | `remove`, `move_to_project`, `add_project`, `remove_project`, `project_copies`, `forget_recent`, `save_environment`, `forget_environment`, `start_environment`, `project_goals_auto`, `project_goal_add`, `project_goal_remove`, `project_goal_resume`, `project_goal_add_crashes`, `edit_project_memory`, `rewind_restore`, `rewind_redo`, `rewind_recover`, `create_chat_group`, `save_chat_group`, `add_chat_group_files`, `remove_chat_group_file`, `remove_chat_group`, `move_chat_to_group`, `new_group_chat`, `forget_auto_learned`, `save_settings` (non-security fields), `save_theme`, `tools_set_enabled`, `set_mcp_server`, `stop_background`, `update_restart` |
| `person` | Only the person on the screen (`error.reach.person` for everyone else) | Sign-in and CLI install (`setup_install`, `setup_sign_in`), cards and approvals (`answer`, `answer_question`, `approve_plan`, `answer_plan`, `approve_credits`, `parked_seen`), confirming project checks (`project_proof_confirm`), catalog installs and removals (`tools_install`, `tools_prepare_replacement`, `tools_replace`, `tools_remove`, `tools_restore`, `tools_clear_removed`), native pickers and external apps (`open_folders`, `show_project`, `show_file`, `save_file`, `open_link`, `open_terminal`, `open_theme`, `export_conversation`), and security fields in `save_settings` (`permissions`, `userHooks`, `faberTrust`, `grantsEpoch`, `permissionsRevision`, `beforePlan`, `previews`, `autostart`) |
| `screen` | Only the screen itself (`error.reach.screen` over the channel) | `state`, `open`, `seen`, `screen_ready`, `input_seen`, `draft_dirty`, `save_notes_seen`, `preview_look`, `set_shell_texts`, `game_in_front`, `set_clock`, `quota_attention`, `save_piece_value`, `visual_open`, `visual_page`, `visual_close`, `site_*`, `cookie_*` |

An AI's **family** is the root conversation found by following `startedBy` up from the AI's own conversation (up to 16
steps), plus every conversation whose `startedBy` chain leads to that same root. When an AI calls `send` without a
conversation id, `send` stamps `startedBy` with the AI's conversation id so the new conversation joins its family.

What the rules do not stop: they keep a well-meant AI on its own work. An AI runs as the same user as the person, so a
program that sets out to break them can read `<data>/channel/secret`, make a token for another conversation, or act as
the person at a terminal (on Unix, with no job to tell it apart, an AI that drops `FABER_AGENT_DEPTH` is taken for the
person). The channel is a wall against other accounts and other machines, not against the person's own programs.

## Conversation states

Callers outside the screen read through `conversations` and `conversation` (`host/src/views.rs`), never `state` or `open`,
so reading from the command line never marks a conversation seen or clears its done card. Each conversation has one state
word:

- `working`: a turn is running or queued to run.
- `waiting`: waiting on the person (a permission card, question, plan approval or quota card).
- `concluded`: the last reply ended cleanly with the agent's own closing box (`wrapUp`) marked `done`.
- `ended`: the last reply ended cleanly without a `done` closing box.
- `interrupted`: Faber closed while the last reply was running (`faber continue <id>` resumes it).
- `stopped`: stopped by the person.
- `failed`: the last reply ended with an error.
- `new`: no assistant reply yet.
- `idle`: what `faber list` (`conversations`) returns for any conversation not `working` or `waiting`, so listing never
  reads conversation files from disk. `faber read <id>` (`conversation`) loads the file and returns the exact end state
  and `conclusion` (the closing box's recap when `concluded`, else `null`).

## Verbs and arguments

Global flags work anywhere on the line: `--json`, `--human`, `--no-start`, `--timeout <s>`.

| Verb | Arguments | What it calls |
| :--- | :--- | :--- |
| `help` | `[<verb or command>] [--json]` | Built-in (`cli/commands.json` + verb table); needs no running Faber |
| `status` | | `hello` offer + `conversations {state: ["running"]}` |
| `list` | `[--state s1,s2] [--archived \| --all] [--project <id>] [--family <id>] [--query <words>] [--limit <n>]` | `conversations` (default limit 50; `--state running` means `working,waiting`) |
| `read` | `<id> [--last <n>]` | `conversation` (default `last: 20`) |
| `send` | `<text \| -> [--to <id>] [--project <id>] [--vendor auto\|claude\|codex] [--model <m>] [--effort low\|medium\|high] [--wait] [--timeout <s>]` | `send`, then `wait` when `--wait` is given |
| `stop` | `<id>` | `interrupt` |
| `continue` | `<id>` | `conversation {id, last: 1}` to find the last message id, then `continue_closed` |
| `archive` / `unarchive` | `<id>` | `archive` (`archived: true` / `false`) |
| `pin` / `unpin` | `<id>` | `pin` (`pinned: true` / `false`) |
| `rename` | `<id> <title>` | `rename` |
| `projects` / `quota` / `settings` | | `projects`, `quota`, `settings` |
| `watch` | `[--id <id>] [--family <id>] [--project <id>]` | Subscribes to channel events and emits state-transition JSON lines |
| `wait` | `<id> [--timeout <s>]` | Waits until `<id>` is no longer `working`, prints `conversation {id, last: 1}` (default timeout 300 s; `0` = none) |
| `call` | `<command> [<arguments as JSON> \| -]` | Any non-`screen` command in `host/src/table.rs` |

## Output, exit codes, `watch` and `wait`

When `--json` is passed or stdout is not a terminal (every AI tool call), `faber` writes one compact JSON value per line
on stdout, and failures write `{"error":"<key>","message":"<English words>"}` on stdout as well. When stdout is a
terminal (or `--human` is passed), `faber` prints short lines in the user's language (`cli.*` in `ui/locales/*.ts`) and
errors on stderr.

| Exit code | Meaning |
| :--- | :--- |
| `0` | Done |
| `1` | Faber refused (`error.*`) |
| `2` | Command-line usage error |
| `3` | Faber is not running and could not be reached or started (`error.channel.*`) |
| `4` | Needs the person (`error.reach.ask`, `error.reach.person`, `error.reach.screen`) |
| `5` | `wait` timed out |
| `130` | Interrupted (`Ctrl+C`) |

`faber watch` subscribes on the channel (`subscribe: true`), emits one line for each conversation currently `working` or
`waiting` (`"was": null`), coalesces engine event bursts into at most one `conversations` read every 250 ms (or reads at
once after `lagged`), and prints a line only when `(state, status, title)` changes:

```json
{"id":"c1","family":"c0","title":"Auth tests","was":"working","state":"concluded","status":null,"conclusion":"All 14 auth tests pass."}
```

When a conversation leaves `working` or `waiting` for `idle`, `watch` calls `conversation {id, last: 0}` so the emitted
line carries its exact end state (`concluded`, `ended`, `stopped`, `failed`, `interrupted`) and `conclusion`.

`faber wait <id>` checks `conversation {id, last: 1}` first; if `<id>` is already not `working`, it prints that view and
exits `0`. Otherwise it watches until `<id>` leaves `working` (including for `waiting`, so an AI never hangs while a card
waits for the person), prints `conversation {id, last: 1}` and exits `0`, or exits `5` on timeout.

## Limits

- At most **16 connections** at once on the channel (the 17th receives `{"id":0,"err":"error.channel.busy"}` and closes),
  **16 requests in flight** per connection, and **1 MiB** per request line (64 MiB for responses).
- An AI caller may have at most **2 turns** it started running at once and start at most **20 turns per hour**
  (`error.channel.agentTurns`); at `agentDepth >= 2` it may start none (`error.channel.agentDepth`).
- Commands marked `Window` in `host/src/table.rs` return `error.host.noWindow` over the channel.

## How an AI uses `faber`

```bash
# Start a sibling conversation in the same project and get its id:
id=$(faber send --project "$proj" "Audit the SQL migrations for lock time" | jq -r .conversationId)

# Do other work, then wait for the sibling to finish or need the person:
out=$(faber wait "$id")
state=$(printf '%s' "$out" | jq -r .state)   # concluded | ended | waiting | failed | stopped

# Tidy up its own family when done:
faber archive "$id"
```

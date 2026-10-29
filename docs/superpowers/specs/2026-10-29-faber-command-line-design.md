# Faber on the command line: one `faber` command for people and AI agents

Status: Direction approved by the owner on 2026-10-29 (answers at the end). Built in the `faber-cli` lane: the channel
(`channel/`), the command (`cli/`), the agent limits and the Windows packaging. Becomes an R-number when it lands.

## Why

While a sign-in bug was being debugged on 2026-10-29, the agent could not see what Faber saw and asked for a
screenshot. The owner refused, rightly: an AI should not need anyone's screen to learn whether Faber thinks Claude Code
is signed in. The owner's words: a CLI "pra os modelos conseguirem acessar, tipo um harness mesmo", that "IAs e pessoas
consigam usar e fazer tudo do faber desktop por lá também", with a very good design.

So the goal has three parts:

1. **Everything the window does, from a terminal.** Chat, Projects, accounts, quota, settings, conversations: the same
   engine, the same rules, the same words.
2. **Two faces of one command.** A person types `faber` and gets a terminal screen as good as the window. An agent or
   a script gets the same commands with JSON in and JSON out, and can drive Faber as a harness.
3. **No screen needed to debug Faber.** `faber status --json` answers what an agent used to ask a screenshot for.

## What the study of Claude Code, Codex and agy says

The CLIs were studied from their `--help`, their published docs, Codex's open source (Apache-2.0) and the extracted
desktop apps in `E:\Dev\re-corpus` (study only, no code copied). The cited findings are in the session notes
(`cli-surfaces-study.md`). What Faber takes from them:

- **One program, many faces.** `claude`, `codex` and `agy` are each one command. With no arguments you get the
  interactive screen. `-p` / `exec` is a one-shot run, and `--output-format stream-json` / `exec --json` prints one
  event per line. `codex app-server` is a long-running JSON-RPC-style server for programs (about 90 methods named
  `area/verb`, an `initialize` handshake, unstable methods hidden unless the client opts in). `claude mcp serve` exposes
  Claude Code as an MCP server. Codex 0.162 has no MCP server mode any more, since `app-server` replaced it. Faber
  follows the same shape.
- **The desktop app is a client of the public protocol.** Codex Desktop runs `codex app-server` and uses the same
  protocol as everyone else, with no private channel. Claude Desktop starts `claude` with stream-json both ways. No
  desktop app has a CLI mode of its own. Codex's desktop install puts `codex.exe` in its own `bin\` folder, and Faber's
  installer does the same with `faber` (below).
- **A user-only channel, never an open port.** Codex's control socket lives in a folder only the user can open (0700
  on Unix, a user-only access list on Windows), and the server also checks that the caller is the same user and not
  elevated. `codex app-server proxy` lets any stdio client reach the running server. A WebSocket listener refuses any
  `Origin` header and any address beyond loopback without a token. Antigravity is the one to avoid here: it puts its
  CSRF token on a child process's command line, where any other program can read it.
- **A schema derived from the code.** Codex generates its protocol's JSON Schema from its Rust types
  (`app-server generate-json-schema`), keeps the result in the repository and fails a test when the two drift. Faber
  prints its table from the running app instead (`faber commands --json`), so it can never drift from what answers.
- **None of them returns credentials** through its programmatic side, and headless runs either refuse open approvals
  or answer them from a closed list of decisions.

## The decision to take: how the command reaches Faber

| Option | What it means | Verdict |
| :--- | :--- | :--- |
| A. Talks to the running Faber | `faber` connects to the Faber that is already open, and starts it hidden in the tray when it is not | **Recommended** |
| B. Its own headless engine | `faber` runs a second copy of the engine with no window | No: two engines on the same conversations and CLI sessions would fight over files and quota |
| C. Reads Faber's files directly | `faber` opens the data folder itself | Only as a fallback for read-only status when Faber cannot start |

Option A costs almost nothing: Faber hidden in the tray already measures 4 MB with the engine running
([architecture](../../product/architecture.md#architecture)). So "headless Faber" is Faber in the tray, which already
exists. There is one engine, and the window, the terminal screen and every agent are clients of it. Whatever one does,
the others see at once. A conversation started in the terminal shows up in the window, and the reverse.

This is also the shape the Electron move needs: when the engine becomes a separate process, the Electron window talks
to it through this same channel. The command line is then not a second interface to maintain, just one more client.

## Shape

```text
faber.exe (window, tray, engine)  ── local channel (same user only) ──┬── faber            (terminal screen, people)
                                                                      ├── faber <verb>     (one command, text or --json)
                                                                      ├── faber rpc        (JSON-RPC over stdio, harness)
                                                                      └── faber mcp        (MCP server: Faber as tools)
```

- **Program.** `faber.exe` is a window program (`windows_subsystem = "windows"`), so a terminal cannot read its output.
  A small console program, `bin\faber.exe`, ships beside it (the Windows release builds it with
  `scripts/cli-bin.mjs` and `src-tauri/tauri.cli.conf.json`). On start, Faber adds that `bin\` to the user's PATH the
  way it does for the CLIs it installs (`src-tauri/src/command_line.rs`); a dev or test build has no `bin\` and changes
  nothing. It holds no engine: it connects, sends, prints and exits. macOS and Linux packages come with the Electron
  installer, where `faber` on Linux is already the app's own name.
- **Channel.** On Windows, a named pipe whose access list allows only the signed-in user, and the server checks the
  caller is that user (the client checks the server the same way). The pipe's name comes from the data folder
  (`faber-<hash>`), so a test Faber with its own folder has its own channel, and only the first instance may create it.
  On macOS and Linux, a Unix socket in `<data>/channel/`, a folder only its owner can open, with the peer's user checked;
  a socket left by a crash is cleared and a live one refuses a second Faber. No token is ever passed on a command line.
  Never a TCP port, never reachable from another machine or a web page. One JSON object per line:
  `{"id":1,"cmd":"state","args":{}}`, answered `{"id":1,"ok":...}` or `{"id":1,"err":"error.key"}`, and events as
  `{"event":{...}}`. Two commands belong to the channel: `hello` (who asks; the answer is the version, the protocol
  level and the command table) and `subscribe` (events start on this connection).
- **Not running.** `faber` starts Faber hidden in the tray (the same windowless launch as autostart's `--autostart`)
  and waits up to 10 seconds for the channel. `--no-start` never starts it.
- **Who serves it.** Whatever process hosts the command table (`host/`, `Host::dispatch`): today the Tauri shell,
  after the move the Electron one. The channel is a thin adapter over it and never holds a second copy of a command.

## The fixed command list

There is one list, the one the screen already uses: the host's table (`Host::commands()`, product-map "Shell
commands"), with the same names and the same camelCase arguments as `ui/api.ts`. Each entry says its kind: it reads,
writes, starts a turn or needs the window (the browser pane's box, `screen_ready`, `game_in_front`), and a window-only
command answers `error.host.noWindow`. A new command enters the table once, so the terminal never falls behind the
window. Nothing outside the list can be called: no raw shell, no file reads beyond what a command already
allows, no setting the window does not offer.

| Verb | Window commands behind it | What it does |
| :--- | :--- | :--- |
| `faber status` | `state`, `setup_check`, `update_status` | Version, each CLI installed / signed in / its plan, quota windows, what is running |
| `faber chat [text]` | `send`, `open`, `interrupt`, `answer`, `answer_question`, `approve_plan` | Starts or continues a chat, streams the answer, answers cards |
| `faber list` / `faber show <id>` | `state`, `open` | Conversations, pinned first then newest, and one conversation in full |
| `faber projects` / `faber project add/remove` | `state`, `open_folders`, `remove_project` | Projects |
| `faber quota` | `state` (quotas, usage) | The windows and their resets |
| `faber sign-in <cli>` / `faber install <cli>` | `setup_sign_in`, `setup_install` | The CLI's own sign-in and install, exactly as the window does |
| `faber settings get/set` | `state`, `save_settings` | Only the settings Settings shows, with the same checks |
| `faber watch` | `subscribe` | Prints every event as it happens (one JSON object per line) |
| `faber stop/rename/archive/unarchive/pin/unpin/remove` | `interrupt`, `rename`, `archive`, `pin`, `remove` | The same actions as the window |
| `faber call <command> [json]` / `faber commands` | any table command | Any command of the table, and the table itself |

`faber rpc` is a stdio proxy to the running Faber, the way `codex app-server proxy` is: the same wire as the channel,
with the window's command names. A harness sends `send`, reads `message` events until the turn ends, and answers cards
with `answer`, `answer_question` or `approve_plan`. `faber mcp` is an MCP server over stdio with six tools
(`faber_status`, `faber_list`, `faber_show`, `faber_chat`, `faber_commands`, `faber_call`), so Claude Code or Codex can
register Faber with their own `mcp add` command: `claude mcp add faber-app -- faber mcp` (the name `faber` is already
Faber's own tool server inside its conversations).

## Output

- **Text for people, JSON for programs.** Every verb prints readable text. `--json` prints one JSON document, and the
  streaming verbs (`chat`, `watch`) print one object per line. Engine keys (`error.*`, `reason.*`) stay in JSON, and the
  text output turns them into words from `ui/locales/` in the person's language.
- **Exit codes mean something.** 0 done, 1 refused or failed (the key says why), 2 wrong use, 3 Faber could not be
  reached, 4 waiting for a person (a card is open and nobody is at the terminal to answer it), 130 stopped with Ctrl+C.
- **The table is printed, not described.** `faber commands --json` prints the host's table as the running Faber has
  it: names, arguments with their types, and kind. `hello` gives the same table with Faber's version and protocol level.
  A newer Faber keeps answering older clients; a change that cannot raises the level.
- **Errors keep the keys.** With `--json` a refusal is `{"error":"error.key","message":"..."}` on stdout; without it the
  words go to stderr. A chat ends with a `result` line. Logs go to stderr, never stdout.

## The terminal screen (people)

`faber` with no arguments opens a screen with the same parts as the window, in the terminal: the conversation list,
the thread, the box, the running list and quota. It follows the design pages instead of a new style:

- **Same looks.** Colours come from the person's theme in `themes.json`, mapped to the terminal's colours, and fall back
  to 16 colours. `NO_COLOR` and a terminal with no colour get plain text.
- **Same words.** Every text comes from `ui/locales/` (built into a table the Rust program reads), in the language of
  Settings. Nothing in English that the window says in Portuguese.
- **Answers read well.** Markdown is drawn (headings, lists, code with colour, tables cut to the width). Tool steps and
  subagents are one line each that opens on Enter, like the work log. Long output stays folded.
- **Cards stay cards.** A question or an approval shows as a short card with its choices on keys (1, 2, 3, Esc), never a
  y/n line that is easy to miss.
- **Keys people already know.** Enter sends, Shift+Enter makes a new line, Ctrl+C stops the turn (twice to leave), `/`
  opens the commands, Up brings back the last text, Tab moves between list and thread. Mouse works where the terminal
  allows it.
- **Edges walked.** It works at 80 columns and in a 24-line window. With reduced motion, a spinner becomes a still
  "working" mark. Before any sign-in, an empty state says what to do. `--plain` prints line by line for screen
  readers and logs.

The window stays the main place. The terminal screen is for people who live in a terminal and for SSH. Whether to
build it in the first slice or the second is a question for the owner (below).

## Safety

- **No credentials, ever.** The command line only calls engine commands. None of them reads `~/.claude`, `~/.codex` or
  any other sign-in folder, and none will. Sign-in opens each CLI's own flow. "Signed in" comes from what the CLI
  reports (the same check as the window), never from reading its files.
- **Same user, same rules.** Only the person's own account can open the channel. Every command runs through the same
  checks as the window: permission mode, refused folders (R121, R124), the fixed list.
- **Agents inside Faber.** A project agent may use every `faber` command, as the person can (owner's answer). Every CLI
  Faber starts gets `FABER_AGENT_DEPTH=1` in its environment, and so does everything it runs, `faber` included. A
  conversation cannot carry its own depth (one Codex process serves every conversation of an area), so the limit is on
  what marked clients start, all together: at most 2 of their turns run at once and at most 20 start in an hour
  (`channel/src/guard.rs`, `error.channel.agentTurns`). A chain of agents that wait on each other stops at the first
  limit; one that hands on and ends stops at the second. A client that says depth 2 or more reads but starts nothing
  (`error.channel.agentDepth`). Every turn shows in the window's running list like any other.
- **Two reviews.** The channel is security code: it gets two reviews from fresh sessions before it lands (AGENTS.md).

## Memory and processor cost

- **Faber open, `faber` not used:** one waiting pipe or socket listener, no thread spinning, under 1 MB. To be
  measured against the R125 test. The agent limits keep one small set of ids, updated from the `running` event.
- **`faber status`:** a console program of about 3 MB that runs for a fraction of a second and exits.
- **Terminal screen open:** one console process, expected at about 10 MB, redrawn only when an event arrives (no idle
  timer), the same rule as the window's idle cost.
- **Faber was closed:** starting it hidden costs what the tray costs today (4 MB measured). A CLI is still started only
  on the first turn, as in the window.

## Parts

The owner asked for all three in one delivery. They are still built and proven in this order, inside one lane:

1. **Reading (fixes the screenshot problem).** The channel, `bin\faber.exe`, `status`, `list`, `show`, `quota`, `watch`,
   `commands`, `--json`.
2. **Driving.** `chat`, cards, `stop`, projects, settings, `rpc` and `mcp`: the harness.
3. **The terminal screen.** The full screen for people, with looks, locales and keys.

The delivery adds a what's-new entry in every language, product-map rows (a new command, a new stored nothing) and its
checks: `channel/tests/channel.rs` (the wire, the limits, one channel per data folder) and `cli/tests/cli.rs` (the
program end to end against a fake Faber), and the memory measurement.

## The owner's answers (2026-10-29, on cards)

1. Direction: yes, one `faber` command that talks to the open Faber, opening it hidden when it is closed, with a
   screen for people and JSON for AIs (the recommended option).
2. Order: all three parts in one delivery ("Tudo junto"), not reading first.
3. The AI working inside a Faber project may drive Faber through `faber` in full ("Pode mexer em tudo"), not only
   read. The depth limit above is a detail decided by AI.

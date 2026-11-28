# Antigravity: keeping a folder out

What the Antigravity CLI's ACP server (`agy acp` 1.5.14, [antigravity-cli.md](antigravity-cli.md#over-acp)) lets a client refuse
before it happens, recorded on 2026-11-22 and 23. The recordings are `tests/fixtures/agy/acp-canary-*.jsonl` and
`acp-guard-*.jsonl`. Every scenario ran once, with harmless files, so each line is what one run did, not a guarantee.

In the recorded runs, in `default` mode, a native file access outside the working folder and outside the system temp
folder asked first and the client refused it. Inside the working folder, and in the system temp folder, the recorded
runs did not ask.

- **Outside the working folder, `default` mode** (`acp-guard-outside`, `acp-guard-write-junction`, `acp-guard-tools`).
  `read_file` and `write_file` on a path outside the working folder sent `session/request_permission` first (the call goes
  `in_progress`, `pending`, then the request); after `reject_once` the call ended `failed`, no content came back
  (whether the CLI opened the file cannot be seen from outside) and the file was not written (looked for on disk after the
  run; a recording holds the messages only). A
  read and a write through a junction inside the working folder that points outside asked too. That request's `args`
  hold the junction's path and its `locations` the real one; the `tool_call` announcement before it holds the
  junction's path in both. A client that judges a path resolves it itself, and refuses when it cannot.
- **Inside the working folder the runs did not ask** (`acp-tools`). A native read and a write there ran with no request,
  so a protected folder that sits inside the project, as a real folder and not a junction, is not known to be kept out.
- **The system temp folder did not ask** (`acp-guard-temp`). The CLI's changelog says it grants itself read access
  there; a read and a write there ran with no request, in `default` mode. The six `acp-canary-*` recordings kept their
  files there, which is why they show outside reads and writes not asking, also under `toolPermission: "strict"`. The
  folder is the child's `TEMP`: `acp-guard-outside` ran with `TEMP` and `TMP` pointed at another folder (the recorder's
  doing, not in the recording) and its files, under the PC's real temp folder, asked. Not tried: a junction under the
  temp folder that points elsewhere.
- **Rules.** `permissions.deny` and `permissions.ask` rules (`read_file(<path>)`, `write_file(<path>)`, `command(...)`,
  `mcp(...)`, in `settings.json` and in `config/config.json`, with and without `--dangerously-skip-permissions`) changed
  nothing in six tries, all on files in the temp folder. Not tried on files outside it or inside the working folder.
- **`auto` mode** (`acp-guard-auto`). An outside read ran unasked and returned the file; an outside write asked and was
  refused. So `auto` cannot keep a folder from being read.
- **`--sandbox`** (`acp-guard-sandbox`, `acp-guard-sandbox-command`). "Terminal restrictions" only: a native outside
  read asked as without the flag, a command asked too (`BypassSandbox: false`), and once allowed that command read the
  file outside the working folder. The sandbox did not keep a folder out of an approved command's reach.
- **Cancelling is not a block** (`acp-guard-cancel`). `session/cancel` sent on the first `tool_call` announcement of a
  write that does not ask: the call reported `failed`, the prompt `cancelled`, and the file was on disk all the same
  (looked for after the run).
- **Other tools.** The only native tools any recording shows are `read_file`, `write_file`, `run_command` and
  `ask_question`. Asked to list a folder with a native tool, the model ran a command, which asked (`acp-guard-tools`);
  asked to search it after that refusal, it declined without calling a tool. In a fresh session (not kept) it said it has
  no native listing or search tool. A native listing or search tool, if one exists, is untested. No recording holds an
  `fs/*` or `terminal/*` request, though the client offered both capabilities in `acp-canary-fs`: the client saw an access
  coming only in the `tool_call` announcement, where cancelling did not stop it, and in a permission request, where a
  refusal did. A command and an MCP tool asked in `default` and `accept-edits`; the client sees a command's text and an MCP tool's arguments, not the
  files either will touch.
- **Settings.** `agy --settings` (no model call) accepts `allowNonWorkspaceAccess`, `enableTerminalSandbox`,
  `toolPermission` (`strict`, `proceed-in-sandbox`, `always-proceed`, `request-review`), `agentMode` and `permissions`,
  and rejects the trusted-folder and policy names that were guessed; `agy acp` takes no `--settings`. Not tried: their
  values other than `strict`, and native outside access in `accept-edits` and `plan`.

## What this means for a driver

None of this is a guarantee: it is what single runs of one CLI version did. What they support: run every conversation in
`default` mode, never `auto`; give the child a temp folder of its own, outside every protected folder; answer each
request in the host, which resolves the path, refuses a protected folder and may answer yes by itself where the person
chose not to be asked. What stays unprotected, and no setting found changes it: a protected folder inside the working
folder, whatever sits under the child's temp folder, a native listing or search tool if one exists, a junction
retargeted between the host's check and the CLI's access, and whatever an approved command or MCP tool reaches, named
or not: the host judges only the text it is shown, and an approved command read the test file by name under `--sandbox`.

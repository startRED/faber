# The Antigravity CLI, as recorded

What `agy` 1.4.10 did on Windows 11 on 2026-10-30, recorded with the real CLI on the owner's subscription so a driver can be
built from facts. The recordings are in `tests/fixtures/agy/`, one scenario per file, in the format of the other recordings: one
`{"dir": ..., "msg": ...}` object per line, where `dir` is `out` (a JSON line agy printed), `in` (a line written to its stdin),
`stdout` or `stderr` (plain text) or `exit` (the exit code). The fake CLI (`testkit/src/fake.rs`) replays a file of that folder
as `agy -p` printed it.

Every run used `--model gemini-3.8-flash-low --effort low --print-timeout 120s` in an empty scratch folder, except where a
scenario changes one of them (a wrong model, a short timeout). Paths became
`<work>`, `<home>` or `<scratch>`, conversation ids were replaced by synthetic ones, and the owner's memory, which the CLI put at
the start of every reply, was dropped. Text deltas were re-split after that trimming, so their boundaries are not real; the fields
and their order are.

## Output

With `--output-format stream-json` every line is one event:

- `init` first: `conversation_id`, and `init` with `model`, `cwd`, `tools` (60 fixed names) and `permission_mode`
  (`request-review` by default and also under `--mode accept-edits`, `always-proceed` with `--dangerously-skip-permissions`).
- `step_update`: `conversation_id`, `step_index`, `state` (`ACTIVE`, `DONE`, `ERROR`) and `step_type`: `user_input`,
  `agent_response`, `tool`, or `system_message` (seen on a resume). An `agent_response` streams `text_delta` while `ACTIVE`; its
  `DONE` carries `duration_seconds` and `usage` (`input_tokens`, `output_tokens`, `thinking_tokens`, `cache_read_tokens`,
  `total_tokens`): one model call each.
- `result` last: `status` (`SUCCESS` or `ERROR`), `response`, `duration_seconds`, `num_turns`, `usage`, and when they apply
  `denied_actions`, `finished_reason` and `error`. Its usage and `num_turns` count the whole conversation, earlier turns
  included: add up the `DONE` steps of a turn, never results.

## The eight questions

1. **Plain turn and tools** (`agy-plain`, `agy-tools`). A tool is a `tool` step: `ACTIVE` with `tool_name` and
   `tool_info.parameters`, then `DONE` (or `ERROR`) with `tool_info.output` or `tool_info.error` (`type: "TOOL_ERROR"`). The
   names the agent uses are not the init list: `read_file` (`Path`; the output is "1 lines, 6 bytes", never the content),
   `write_file` (`Path` only: no content and no diff anywhere in the stream), `run_command` (`CommandLine`; the output holds the
   exit code and the command's text; the model wraps commands in `cmd /c` or `powershell -NoProfile -Command`) and
   `call_mcp_tool`. A changed file shows only as its path; a driver that shows changes reads the files itself.
2. **Approval** (`agy-approval-default`, `agy-approval-accept-edits`, `agy-approval-stream-json`, `agy-approval-allow-rule`). In
   print mode nothing asks: a command is denied at once in the default mode and under `--mode accept-edits` alike, the turn ends
   with `SUCCESS` and exit 0, and the denial shows only in `result.denied_actions` (`{"action":"command"}`) and in a stderr
   line. A line written on stdin changes nothing, and nothing waits for an answer. A command runs with
   `--dangerously-skip-permissions` or an allow rule: `--settings '{"permissions":{"allow":["command(*)"]}}'` (only `command(*)`
   was tried). An edit under accept-edits was not tried.
3. **Two turns in one process** (`agy-two-turns`, `agy-stream-input-bad`). `--input-format stream-json --output-format
   stream-json` without `-p` reads `{"event":"user","message":{"content":"..."}}` lines; `init` comes once, before any input.
   Each turn ends with its `result` and the process waits for the next line. Other shapes end the run with exit 1 and a
   `result` of `status: "ERROR"` (a missing `event`, a missing `message`, a `message` that is not an object, no `content`); an
   unknown event name only warns on stderr. Closing stdin ends the process with exit 0. The only child process is a configured
   MCP server's; it was gone 2 s after agy was killed without `/T`, but the test server ends with its stdin, so whether agy stops
   its children is not verified.
4. **Resume** (`agy-resume`, `agy-resume-missing`). `--conversation <id>` keeps the id and the history in a new process (the
   model quoted the first message) and prints `init` again; `step_index` goes on. An unknown id is not an error: exit 0, a
   stderr warning (`conversation "..." not found`) and a new conversation, so a driver compares the id in `init` with the one it
   asked for.
5. **Errors** (`agy-wrong-model`, `agy-wrong-model-effort`, `agy-print-timeout`, `agy-unknown-setting`). An unknown model exits
   1 with one `result` line (`status: "ERROR"`, empty `conversation_id`, the list of models by display name in `error`), the
   same words on stderr, and no model call. `--print-timeout` ends with exit 0 and `finished_reason: "print_timeout"` with a
   partial response. Not verified: signed out (a fresh profile folder stays signed in, and the real sign-in was not touched;
   `agy-no-credentials-proxy` is only the API-key provider without its key), a spent quota (none was spent), and the exit
   code 3 with an `AGY_ERROR:` stderr line that the changelog names for failures under `-p`.
6. **Isolation** (`agy-isolated-home`). `--settings '{}'` and `--disable-slash-commands` still load the owner's memory and
   skills (the first call reads about 19k tokens and the model says it has memory about the user). `--settings` accepts only
   known keys (an unknown one exits 1), and none covers memory, rules, skills, plugins, hooks or MCP. What works: `USERPROFILE`
   set to an empty folder. The model then has no memory, the first call reads 6.8k tokens, the tools are the same, and the CLI
   stays signed in (its sign-in lives outside that folder; where was not looked at). It recreates `.gemini/` there with its
   built-in skills. Files in the working folder (`.agents/`, `GEMINI.md`, `AGENTS.md`) are still read by design.
7. **MCP for one session** (`agy-mcp-stdio`). `--settings` refuses `mcpServers`. A stdio server works from
   `<profile>/.gemini/config/mcp_config.json` in an isolated profile (`agy mcp add` writes it:
   `{"mcpServers":{"name":{"command","args","env","disabled":false}}}`) or from `<work>/.agents/mcp_config.json`. Its tools
   never appear in `init`; a call shows as a `call_mcp_tool` step (`ServerName`, `ToolName`, `Arguments`). The server is started
   when first needed and must be allowed (skip-permissions here; the allow-rule form for MCP tools was not tried).
8. **Quota**. No zero-token way was found. `agy models` lists ids only, `-p "/usage"` goes to the model as an ordinary prompt,
   `/usage` and `/credits` exist only in the interactive screen, and the CLI's log shows the quota fetched over the network. No
   local file name holds it.

## What the CLI keeps

Under `<profile>/.gemini/` (the real profile, or the `USERPROFILE` override): `antigravity-cli/` with `conversations/<id>.db`,
`brain/<id>/` (transcripts, chunks, scratch), `annotations/`, `presence/<id>.lock`, `log/`, `crashes/`, `cache/`, `builtin/skills/`,
`conversation_summaries.db`, `installation_id` and an `updater/`; and `config/` with `mcp_config.json` and
`projects/default-cli-project.json`. Every run adds a conversation there, so a Faber session in an isolated profile also keeps
its history out of the person's own list. The recording runs left about twelve conversations in the owner's real profile.

## Cost of the recording

About 36 model calls, all `gemini-3.8-flash-low` at low effort. The 29 whose output was kept read 127,314 input tokens (plus
319,334 cache reads) and wrote 3,094 output and 2,005 thinking tokens: 130,408 in all, well under US$5.

## Over ACP

What `agy acp` 1.5.14 did on Windows 11 on 2026-11-23: the same CLI as a long-lived server that speaks the Agent Client Protocol,
one JSON-RPC message per line on stdin and stdout. The recordings are `tests/fixtures/agy/acp-*.jsonl`. A line agy printed is
`{"dir": "stdout", "msg": "<the JSON line as text>"}`, as in the `acct-*` files (`out` is kept for print-mode events); `in` is a
message the client wrote; `exit` is the exit code, or `"killed"` when the recorder killed the process. A file with more than one
`exit` holds several processes in a row. The flags a process was started with are not in the file; they are named below.
Nothing came on stderr in any run, and closing stdin ends the process with exit 0.

The client named itself `faber` and worked in an empty scratch folder on `gemini-3.8-flash-low`. Paths, ids and call ids were
replaced as above; the owner's slash commands were dropped from `available_commands_update` (the 13 built-in ones stay) and
the owner's other conversations from `session/list`. A reply chunk that held a path was re-split after the replacement.

**Start.** `initialize` answers `protocolVersion: 1`, `agentInfo` ("Antigravity ACP Server", the version), `agentCapabilities`
(`loadSession`; prompts with `image` and `embeddedContext`; sessions with `resume`, `close`, `load`, `list`,
`additionalDirectories`; `auth.logout`; `_meta.antigravity` with `tier` and `systemPrompt.append`, the latter not tried) and
four `authMethods` (`oauth-personal`, `oauth-business`, `gemini-api-key`, `agent-platform`). `session/new {cwd, mcpServers}`
answers `sessionId`, `models` (23 ids such as `gemini-3.8-flash-low`; `currentModelId` is `gemini-4-neon-high`), `modes`
(`default`, `accept-edits`, `auto`, `plan`), `configOptions` (`model` by family id such as `gemini-3.8-flash`, `thought_level`,
`mode`) and `_meta.antigravity.tier`. Sign-in was never needed and `agy acp --login` was never run; signed out is not verified.

1. **Plain reply** (`acp-plain`). `session/prompt {sessionId, prompt: [{type: "text", text}]}` streams `session/update`
   notifications: `available_commands_update`, `usage_update` (`used`, `size`: the context window), `session_info_update`
   (`title`) and `agent_message_chunk` (`content.text`). The turn ends with the request's result: `stopReason: "end_turn"` and
   `usage` for that turn alone (`inputTokens`, `outputTokens`, `cachedReadTokens`, `cachedWriteTokens`, `thoughtTokens`,
   `totalTokens`). No thinking text was seen at low effort.
2. **Tools** (`acp-tools`). A `tool_call` update opens a call (`toolCallId`, `title`, `kind`, `status`, `locations`, `rawInput`,
   and the tool's name) and `tool_call_update` moves it through `pending`, `in_progress` and `completed` or `failed`. Seen:
   `read_file` (kind `read`, `Path`; no content comes back), `write_file` (kind `edit`, `Path` and `Content`; the update carries
   `content: [{type: "diff", path, newText}]`, with no `oldText` for a new file), `run_command` (kind `execute`, `CommandLine`;
   `rawOutput` has `output` with the exit code and the command's text, and `exit_code`), `mcp_<server>_<tool>` (no result in the
   update) and `ask_question`. Unlike print mode, a changed file arrives with its new text.
3. **Approval** (`acp-approval-allow`, `acp-approval-deny`, `acp-mode-accept-edits`, `acp-mode-auto`, `acp-skip-permissions`).
   agy sends the request `session/request_permission` (its `id` is a string) with the `toolCall` and two options, `allow`
   (`allow_once`) and `deny` (`reject_once`); there is no "always" option. The answer is
   `{outcome: {outcome: "selected", optionId}}`. After `deny` the call is `failed` with empty output and the turn ends
   `end_turn` with no reply text. In `default` and `accept-edits` a command and an MCP tool ask; reading and writing files do
   not, inside or outside the working folder. In `auto` the command ran without asking (the mode's description says it still
   asks for irreversible actions; none was tried), and with the flag `--dangerously-skip-permissions` nothing asks.
   `session/set_mode {sessionId, modeId}` answers `{}` and a `current_mode_update`; an unknown mode is error -32602. So Faber's
   Ask is `default`, and Don't ask is `--dangerously-skip-permissions` (or `auto`, which may still ask). `plan` was not tried.
4. **Questions** (`acp-question`, `acp-question-elicitation`, `acp-question-declined`). `ask_question` is a tool call with
   `rawInput.questions` (`question`, `options`, `is_multi_select`). To a client that declares
   `clientCapabilities.elicitation.form`, agy sends the request `elicitation/create` (`sessionId`, `toolCallId`,
   `mode: "form"`, `message`, and a `requestedSchema` with one choice field per question, `q1` with numbered options, plus
   `q1_other` for free text). `{action: "accept", content: {q1: "2"}}` gave the model the second option; `{action: "decline"}`
   completes the call and the model says the question was declined. Without the capability the call stays `pending`, the
   question arrives as reply text, the turn ends, and the next `session/prompt` is taken as the answer. A multi-select
   question and more than one question at a time were not tried.
5. **Stop** (`acp-cancel`, `acp-cancel-pending`). The notification `session/cancel {sessionId}` during a running `ping` fails
   the tool call and ends the prompt with `stopReason: "cancelled"`; the `ping` process was gone 3 s later, and the next prompt
   in the same session worked. During a pending approval the client also answers it with `{outcome: {outcome: "cancelled"}}`;
   the result is the same and the command never ran. A grandchild process was not tried.
6. **Resume** (`acp-resume`, `acp-resume-unknown`). In a new process `session/load {sessionId, cwd, mcpServers}` replays the
   conversation as `user_message_chunk` and `agent_message_chunk` updates and answers like `session/new`; the next prompt
   remembered the earlier turn. `session/resume` does the same without the replay. Both put the model and the mode back to the
   defaults, so a driver sets them again. An unknown id: `session/load` succeeds with an empty session under that id (no
   error and no replay, so the answer alone does not tell), `session/resume` is error -32603 ("Failed to read session") and
   `session/prompt` is -32602 ("Session not found"). `session/list {cwd}` returns the conversations of that folder with
   titles, the owner's own included; `session/close` answers `{}`.
7. **Failures** (`acp-killed`, `acp-model`). With agy killed in the middle of a command, the command's process was gone 3 s
   later; loading that session in a new process replays the prompt and the tool call as `failed`, with no reply. An unknown
   model is error -32602 ("Unknown model ID") and an effort the model lacks is -32602 ("Thinking level ... is not offered for
   model ..."), both with no model call. Not verified: signed out, a spent quota, the network down.
8. **Model and effort** (`acp-model`, `acp-new-params`). `session/set_model {sessionId, modelId}` with an id of
   `models.availableModels` (model and effort in one, such as `gemini-3.8-flash-low`) answers `{}`; or
   `session/set_config_option {sessionId, configId, value}` with `model` (the family id) and `thought_level` (`low`, `medium`,
   `high`, and `xhigh` where offered) separately, which answers the whole `configOptions` list. `session/new` ignores a model,
   a mode or config options in its parameters, so a session always starts on `gemini-4-neon-high`: set the model before the
   first prompt.
9. **Protecting a folder** (`acp-canary-ask`, `acp-canary-indirect`, `acp-canary-strict`, `acp-canary-rule2`,
   `acp-canary-rule-noskip`, `acp-canary-fs`). It cannot be done from the client. With harmless files in a folder outside the
   working folder, in `default` mode and an isolated profile, `read_file` read one and `write_file` wrote one with no
   permission request; the same through a junction to that folder and when the path came from a file in the working folder.
   `toolPermission: "strict"` in `settings.json` made a write inside the working folder ask (the request carries the diff),
   yet the outside read and write still ran unasked. `permissions.deny` and `permissions.ask` rules (`read_file(<path>)`,
   `write_file(<path>)`, `command(...)`, `mcp(...)`, in `settings.json` and in `config/config.json`, with and without
   `--dangerously-skip-permissions`) changed nothing in six tries. agy ignores the client's `fs` and `terminal` capabilities:
   it never sends `fs/*` or `terminal/*` requests, so the client never sees a file access before it happens. Only a command
   and an MCP tool can be refused, and only in `default` and `accept-edits`. Not tried: `--sandbox`, and rules in any other
   place.
10. **MCP servers** (`acp-mcp-stdio`). `session/new` takes `mcpServers: [{name, command, args, env: []}]`. The server is
    started at once, its tool is named `mcp_<server>_<tool>`, a call asks for permission in `default` mode, and its result
    reached the model. Only stdio was tried; `env` with values and the HTTP form were not.
11. **The owner's memory and skills** (`acp-iso-gemini-dir`, `acp-iso-app-data-dir`). They load by default: 39 slash commands
    and about 11.6k input tokens on the first call. `agy acp --gemini-dir <an empty folder>` stays signed in and loads none of
    it: the 13 built-in commands and about 8.7k tokens. Overriding `USERPROFILE` does the same, and
    `--include-default-customizations=false` changed nothing (neither recording was kept). `--app-data-dir` refuses an
    absolute path (`session/new` fails with -32603, "must not be absolute"; not in the recording); a relative name works as a
    subfolder of the gemini folder (tried only together with `--gemini-dir`). In that folder the settings file is
    `antigravity-cli/settings.json`.
12. **Plan and quota** (`acp-new-params`, and every `session/new`). `_meta.antigravity.tier` in the answer to `session/new`,
    `session/load` and `session/resume` holds the plan (`name`, `id`, `upgradeUri`) with no model call. No quota numbers
    exist over ACP: `usage_update` is the context window only, and nine guessed method names are error -32601.

### The reader

`engine-drivers/src/agy.rs` reads these messages into the engine's events, with no process: the driver hands `AcpReader`
each line agy prints (`read`) and each message it writes (`sent`), so an answer is read by the request it answers. A reply
chunk is `Text` (after a step, `Break` first); a call is a `Tool` named as the screen draws Claude Code's (`Bash`, `Write`
or `Edit` from the diff, `Read`, `Grep`, `AskUserQuestion`, `mcp__<server>__<tool>` split by the servers given to
`session/new`; any other call keeps agy's name), ended by `ToolResult` with a command's own output and exit code, cut at
`TOOL_OUTPUT_CHARS`, and `declined` after a reject. A permission request and a form question are `Permission` events whose
`request_id` is agy's id; `answer_permission` and `answer_question` build the answer, `cancel_pending` cancels what waits
after a stop. A session's answer, and a model `set_model` took, is `Started`. The prompt's answer is `Done` (input with
its cache, output with its thinking); a stop other than `end_turn` adds its `reason.*` key. An error answer to the prompt
or a setting ends the reply (`reason.fail.rejected` for -32602); one to opening a session, or "Session not found", is
`error.sessionLost`. Updates during a `session/load` are the replay and are dropped. A call still open ends with its turn.

### Cost of the ACP recording

72 model calls. The 69 kept read 535,522 input tokens plus 705,004 cache reads and wrote 3,933 output and 45 thinking tokens
(1,244,459 by the CLI's own total); three overwritten ones add about 35,000. All ran on `gemini-3.8-flash-low` except two that
ran on the default `gemini-4-neon-high` before the model was set (the first plain reply, redone, and the prompt after the
load in `acp-resume`). The runs outside an isolated profile left their conversations in the owner's real profile.

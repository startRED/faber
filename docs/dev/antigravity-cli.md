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
   `end_turn` with no reply text. In `default` and `accept-edits` a command and an MCP tool ask. The files that were read
   and written without asking were in the working folder or the child's temp folder. Later runs confirmed that native
   reads and writes outside both folders ask in `default` mode. In `auto` an outside read and the recorded command ran
   without asking, while an outside write asked. See [folder experiments](antigravity-folders.md).
   `session/set_mode {sessionId, modeId}` answers `{}` and a `current_mode_update`; an unknown mode is error -32602.
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
9. **Protecting a folder**: [antigravity-folders.md](antigravity-folders.md). In the recorded runs (one
   each), in `default` mode a native read or write outside the working folder and the child's temp folder asked first and
   was refused; inside either, none asked; `auto` read outside unasked; `--sandbox` and cancelling did not stop an access.
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

`AcpReader::read` and `sent` pair messages and produce the engine's reply, tool and permission events. A rejection marks
the tool declined. Prompt answers report usage and complete the turn. Errors and interrupted turns provide existing
reason keys. The reader drops history replayed during `session/load` and closes unfinished tool calls at turn end.

### Cost of the ACP recording

72 model calls, 69 kept: 535,522 input tokens, 705,004 cache reads, 3,933 output and 45 thinking. Three overwritten
calls add about 35,000. Two used the default Neon model and the rest used Flash Low. Unisolated runs kept their
conversations in the owner's profile. The eight isolated guard recordings took about 22 calls on Flash Low. Their
14 kept prompts used 179,600 input tokens (80,866 cached) and 4,484 output tokens.

## The session driver

`start_conversation_agy` calls `agy::session::start` with the resolved executable. One owned process per live conversation
runs `agy acp --gemini-dir <data>/antigravity/profile` with client name `faber`. Each process gets a new temp folder under `<data>/antigravity/tmp`, removed when its end is confirmed. The child does not inherit `GEMINI_API_KEY`, `GOOGLE_API_KEY`, `GOOGLE_APPLICATION_CREDENTIALS`, `GOOGLE_GENAI_USE_VERTEXAI`, `GOOGLE_CLOUD_PROJECT`, `GOOGLE_CLOUD_LOCATION`, `GCLOUD_PROJECT`, `GOOGLE_CLOUD_QUOTA_PROJECT`, `GOOGLE_OAUTH_ACCESS_TOKEN` or `CLOUDSDK_AUTH_ACCESS_TOKEN` (removed by name, never read). Stop ends the process if `session/cancel` is not answered in time. Idle close ends the process tree. The next
turn uses `session/resume` (an unknown id is an error there, not an empty session) and checks the id it answers.

Ask and Full access both use `default` so outside file reads still ask the host. The host answers ordinary approvals
in Full access and shows the agent's questions in both modes. Other permission modes return `error.notConnected`.
A file request shows its resolved path (junctions followed); an unresolvable one is refused.

An empty model leaves the CLI default. A listed model id uses `session/set_model`. Otherwise the driver sets `model` and
then `thought_level` with `session/set_config_option`. Resume does the same. The out-of-quota recogniser
looks for `RESOURCE_EXHAUSTED` in an error answer. This is a guess exercised by a synthetic fixture because no real
quota error was recorded; it marks the account out and the reply ends with `error.quotaOut`. The session cannot block access to a protected folder inside the working folder, inside its own temp
folder, within an approved command's or MCP tool's reach, behind a junction changed after the request was judged, or
named by a tool whose input carries no `file_path` (`PROTECTED_FOLDER_LIMIT`). See the [folder experiments](antigravity-folders.md).

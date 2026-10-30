# The Antigravity CLI (Gemini), as recorded

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

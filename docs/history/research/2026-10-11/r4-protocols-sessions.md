# R4: Claude Code and Codex protocols, sessions and control (full notes)

Date seen: 2026-10-11 (all web facts fetched that day). Local versions: Claude Code 2.1.296, codex-cli 0.162.1.

Source notes:
- Claude docs: code.claude.com (fetched). Some pages were huge and were read in chunks; cited sections are exact.
- Codex docs: developers.openai.com/codex/* answers with 308 redirects to learn.chatgpt.com/docs/* (followed; same OpenAI docs). Pages that 404'd are marked.
- Codex protocol: generated locally with `codex app-server generate-json-schema --out <scratchpad>/schema2` (codex-cli 0.162.1). Schema files: `schema2/codex_app_server_protocol.v2.schemas.json`, `schema2/ClientRequest.json`, `schema2/ServerNotification.json`, `schema2/ServerRequest.json`, `schema2/CommandExecutionRequestApprovalResponse.json`.
- `claude --help`, `claude --help` flag list, `codex --help`, `codex exec --help`, `codex app-server --help`, `codex resume --help`, `codex app-server daemon --help`, `codex agents --help` were read locally.
- No credential files (`~/.claude` auth, `~/.codex` auth) were read. Some WebFetch outputs were saved by the harness under `C:\Users\Erick\.claude\projects\...\tool-results\` and read back; they contain docs text only.

---------------------------------------------------------------------
## CLAUDE CODE (2.1.296 local)

### 1. Session lifecycle

Sources:
- https://code.claude.com/docs/en/sessions (Manage sessions)
- https://code.claude.com/docs/en/headless (Run Claude Code programmatically)
- https://code.claude.com/docs/en/cli-reference
- https://code.claude.com/docs/en/agent-sdk/python (ClaudeAgentOptions)

Facts:
- New session: `claude -p "<prompt>"`. `--session-id <uuid>` sets the ID (must be a UUID). `-n/--name` sets a display name.
- Resume: `--resume <session-id | name | absolute .jsonl transcript path>`, `-r`. Without an arg it opens the picker. ID search covers the current project and git worktrees, then every other project on the machine (exactly one match required for cross-project).
- Continue: `--continue/-c` loads the most recent conversation in the current directory. Skips sessions created with `claude -p` / Agent SDK and `/loop` sessions. `claude -p --continue` includes `-p`/SDK sessions.
- Fork: `--fork-session` with `--resume` or `--continue` gives a new session ID. In-session `/branch [name]` copies transcript and switches. SDK: `fork_session=True` (Python) / `forkSession: true` (TS) with `resume`.
- `--no-session-persistence` (print mode only): nothing written to disk, not resumable.
- Persistence: JSONL at `~/.claude/projects/<project>/<session-id>.jsonl` (`<project>` = cwd with non-alphanumerics replaced by `-`, >200 chars truncated + hash). Subagent transcripts: `~/.claude/projects/{project}/{sessionId}/subagents/agent-{agentId}.jsonl`. Format is internal and "changes between versions" (sessions doc). Location override: `CLAUDE_CONFIG_DIR`; project dir name: `CLAUDE_CODE_PROJECT_DIR_NAME`.
- Lifetime: retention `cleanupPeriodDays`, default 30 days (sessions doc; subagents doc). Delete: `claude purge`, `claude rm <id>` (background sessions).
- What resume restores: full history incl. tool calls; model; `--agent`; permission mode (terminal `--continue`/`--resume` restore it; `-p --resume` starts in the `-p` default unless the session ended in plan mode under conditions). Active goal, unexpired scheduled tasks. NOT restored: `--mcp-config`, `--settings`, `--plugin-dir`, `--fallback-model`, `--add-dir` (must be passed again). Settings files are re-read at launch.
- Tool call running when process died: not re-run on resume; Claude is told it was cut off. `CLAUDE_CODE_RESUME_INTERRUPTED_TURN=1` continues it instead.
- Resume re-sends full history on the next request. Prompt cache keeps the prefix if unchanged and within TTL (see 5/Cache). Resume after >1h idle is effectively a cold cache (full reprocess once).
- Pro/Max: on resume of a session inactive >~1h and >100,000 tokens, a dialog offers "Resume from summary" (runs /compact), "Resume full session as-is", or "Don't ask me again" (sessions doc, "Resume from a summary").
- System prompt: recorded on the conversation's first request and reused until compaction, including on resume. Use `--system-prompt-snapshot off` to rebuild each time (CLI reference, "System prompt flags in resumed conversations").
- `-p` and SDK sessions: excluded from picker and `--continue`, still resumable by ID.
- `claude -p --resume` with a session id: prints result JSON with `session_id`; `total_cost_usd` on resume is cumulative for the conversation (headless doc).

### 2. Mid-session control

Sources:
- https://code.claude.com/docs/en/interactive-mode (Queued messages; keyboard)
- https://code.claude.com/docs/en/agent-sdk/python (ClaudeSDKClient methods)
- https://code.claude.com/docs/en/agent-sdk/streaming-vs-single-mode
- https://code.claude.com/docs/en/agent-sdk/streaming-output
- https://code.claude.com/docs/en/model-config (effort, /model)
- https://code.claude.com/docs/en/prompt-caching (switching models, effort, permission mode)
- https://code.claude.com/docs/en/headless (SIGTERM / SIGINT)

Interrupt:
- TUI: `Esc` interrupts the turn (keeps queued drafts). `Ctrl+C` interrupts a running operation.
- SDK: `interrupt()` "sends a stop signal but does not clear the message buffer. Messages already produced by the interrupted task, including its ResultMessage, remain in the stream. You must drain them with receive_response() before reading the response to a new query." Only works in streaming input mode.
- `-p` process: SIGINT ends the turn cleanly. SIGTERM exits with code 143, leaves the turn unfinished with no result recorded, kills running Bash process trees, runs SessionEnd hooks only.

Steer / queue while a turn runs:
- TUI: Enter while Claude works queues the message. A queued message is passed to Claude "as soon as those tool calls finish, within the same turn." Commands/`!` shell commands are held until the turn ends. `Ctrl+Enter` (v2.1.275+) sends queued messages now and interrupts only if Claude is writing a response or running something that can't background (v2.1.281+). `Up` takes queued items back.
- SDK streaming input mode: "Queued messages: send multiple messages that process sequentially, with ability to interrupt" (streaming-vs-single-mode).
- SDK `ClaudeSDKClient.query()` while a turn is already running: NOT documented (python SDK page only says "pair each query() call with its own receive_response() loop").
- `--max-turns` with `--input-format stream-json`: a message still queued when the limit ends a turn "stays queued and starts a new turn with its own limit" (CLI reference).

Change model mid-session:
- TUI `/model <alias|name>` switches now; `/model` alone opens picker. Picker `Enter` saves as default, `s` = this session only. Subagents that inherit the main model follow the switch.
- Cache: "Switching models recomputes the entire request even when the content is identical" (prompt-caching). When a switch is made with a warm cache the TUI asks to confirm (cache TTL window). `PreModelSwitch`/`PostModelSwitch` hooks exist.
- SDK: `set_model(model | None)` (None resets to default).
- Whether history is kept on `/model`: not stated explicitly (inferred from docs only).

Change effort mid-session:
- `/effort [level|auto]` (also while Claude works). Levels: `low, medium, high, xhigh, max` (`ultracode` requires v2.1.203+; `max` not accepted in `effortLevel` setting).
- Cache: on most models a change invalidates the cache. On Opus 5.5, Sonnet 5.5, Haiku 5.5, Fable 5.1 with API key or subscription the cache is kept by default (not on Bedrock/Vertex/Foundry/gateway, HIPAA).
- Applies from the next request in the turn after the cache-warning confirmation (model-config).
- SDK: `effort` option in ClaudeAgentOptions (no documented setter method on ClaudeSDKClient in the page read).

Change permission mode mid-session:
- TUI `Shift+Tab` cycles. SDK `set_permission_mode(mode)` ("Change the permission mode for the current session").
- Cache: "Switching between permission modes ... does not change the system prompt or tool definitions, so mode changes are cache-safe." Exception: `opusplan` model setting.

Other mid-session things:
- `/compact [instructions]`, `/clear`, `/rewind`, `/branch`.
- Deny rules for a bare tool name: removes tool from requests (cache invalidation unless tool search defers it).

### 3. Permission and sandbox modes; how a harness answers

Sources:
- https://code.claude.com/docs/en/permission-modes
- https://code.claude.com/docs/en/hooks
- https://code.claude.com/docs/en/headless (--permission-prompts)
- https://code.claude.com/docs/en/agent-sdk/python (can_use_tool)
- https://code.claude.com/docs/en/cli-reference (--permission-prompt-tool, --permission-mode)

Modes (CLI value; UI label):
- `default` (Manual): reads only run without asking.
- `acceptEdits`: file edits + common fs commands (mkdir, touch, rm, rmdir, mv, cp, sed) inside working dirs.
- `plan`: reads; blocks edits until a plan is approved.
- `auto`: classifier reviews actions instead of the user. Requires a supported model and account; built-in starting mode for terminal/VS Code from v2.1.283.
- `dontAsk`: anything that would prompt is denied (CI). Not in the Shift+Tab cycle.
- `bypassPermissions`: everything runs, but deny rules still block, protected paths still need approval except here, and "critical path" rm still prompts. Only in isolated containers.
- Alias `manual` = `default` (v2.1.200+).
- Start mode: `--permission-mode` > `permissions.defaultMode` in settings (project `.claude/settings.json` can't set `auto` or `bypassPermissions`) > built-in. Built-in for `claude -p` / SDK: `default` in sessions that fetch feature flags; `auto` in sessions that don't, with v2.1.285+.
- `--allow-dangerously-skip-permissions` adds bypass to the cycle without starting in it.

Answering approval requests as a harness:
- Hooks (`hooks` setting):
  - `PreToolUse` output `hookSpecificOutput.permissionDecision`: `allow | deny | ask | defer` with `permissionDecisionReason`. `updatedInput` under `hookSpecificOutput` rewrites tool args. Exit code 2 blocks the call. Timed-out command/http/mcp_tool hooks do NOT block.
  - `PermissionRequest` fires when a tool needs a decision. Matcher on tool name. Decision via `hookSpecificOutput.decision.behavior` = `allow | deny`; `updatedInput` inside `decision`. Exit code 2 is NOT honored for this event.
  - Whether `PermissionRequest` fires under `-p` was not stated in the page read.
- `--permission-prompt-tool <mcp_tool>`: MCP tool answers prompts in print mode. Cannot approve tools that require user interaction (converted to deny).
- `--permission-prompts host|none` (v2.1.259+): `host` = SDK host / prompt tool answers (default); `none` = anything that would prompt is denied and Claude is told not to retry. Hook `PermissionRequest` can still allow in `none`.
- SDK `can_use_tool` callback: called only when the flow falls through to a prompt. Not called for allowed tools, allow rules, or permission_mode auto-approvals. The docs recommend a PreToolUse hook to gate every call.
- Denials in stream-json: `permission_denied` system messages; final `result` lists `permission_denials`.
- Sandbox (Bash sandbox, macOS/Linux/WSL2): separate from modes. `/sandbox` auto-allow mode. (sandboxing docs not fetched.)

### 4. Streaming events (stream-json)

Sources:
- https://code.claude.com/docs/en/headless
- https://code.claude.com/docs/en/agent-sdk/streaming-output
- https://code.claude.com/docs/en/agent-sdk/python (RateLimitEvent, ResultMessage, SystemMessage)
- https://code.claude.com/docs/en/hooks (StopFailure matchers)
- https://code.claude.com/docs/en/costs

Flags: `-p --output-format stream-json --verbose --include-partial-messages`; input `--input-format stream-json`; `--include-hook-events`; `--forward-subagent-text`; `--replay-user-messages`; `--prompt-suggestions`.

Event types seen:
- `system` subtype `init`: session_id, model, tools, mcp_servers, plugins, optional `capabilities` (e.g. `interrupt_receipt_v1`, `interrupt_cancel_queued_v1`; v2.1.205+). Errors: `mcp_server_errors`, `plugin_errors` keys.
- `system` `api_retry`: attempt, max_retries, retry_delay_ms, error_status, error (category: `authentication_failed, oauth_org_not_allowed, account_on_hold, billing_error, rate_limit, overloaded, invalid_request, model_not_found, server_error, max_output_tokens, cloud_credential_error, unknown`).
- `system` `permission_denied`.
- `system` `hook_started`, `hook_progress`, `hook_response` (SessionStart/Setup always; others with --include-hook-events).
- `system` `plugin_install` (with CLAUDE_CODE_SYNC_PLUGIN_INSTALL).
- `system` `compact_boundary` (Python: SystemMessage subtype `compact_boundary`; TS: SDKCompactBoundaryMessage).
- `system` `session_state_changed` (needs `CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS=1`; field `state`).
- `system` `task_started | task_progress | task_notification` (status completed|failed|stopped).
- `system` `prompt_suggestion`.
- `assistant`, `user` (with `parent_tool_use_id`; null for main thread).
- `stream_event` (only with --include-partial-messages): raw API events `message_start`, `content_block_start`, `content_block_delta` (`text_delta`, `input_json_delta`), `content_block_stop`, `message_delta`, `message_stop`. Stream events only for main session, not subagents.
- `rate_limit_event`: `rate_limit_info` with `status` (`allowed | allowed_warning | rejected`), `resets_at`, `rate_limit_type`, `utilization` (0.0-1.0), `overage_status`, `overage_resets_at`, `overage_disabled_reason`.
- `result`: `subtype` (`success`, `error_during_execution`, `error_max_turns`, `error_max_budget_usd`, `error_max_structured_output_retries`), `is_error`, `session_id`, `num_turns`, `duration_ms`, `total_cost_usd` (client-side estimate), `usage` (`input_tokens`, `output_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens`; main loop only, excludes subagents), `model_usage` (per model; use for accounting), `permission_denials`, `terminal_reason` (`completed, max_turns, api_error, aborted_streaming, aborted_tools`), `stop_reason`, `structured_output`, `errors`, `api_error_status`. In streaming input mode `total_cost_usd`/`model_usage` are cumulative across turns.
- Usage cache fields for statusline: `current_usage`, `prompt_cache` object; `usage.cache_creation` gives `ephemeral_1h_input_tokens` / `ephemeral_5m_input_tokens`.
- `StopFailure` hook (API-error turn end) carries the same error categories as `api_retry`.
- Stream cut off: `message_stop` still arrives on interrupt; a cut tool-use block gets no `content_block_stop`. Before v2.1.290 a cut stream could skip `message_stop`.

Cost and budget:
- `--max-budget-usd` stops the run at a client-side estimate; subagent spend counts; restored totals from earlier runs don't count toward it.

### 5. Context management

Sources:
- https://code.claude.com/docs/en/model-config (auto-compact window)
- https://code.claude.com/docs/en/prompt-caching (compaction and cache)
- https://code.claude.com/docs/en/agent-sdk/python (get_context_usage)
- https://code.claude.com/docs/en/hooks (PreCompact/PostCompact)
- https://code.claude.com/docs/en/cli-reference (--autocompact)

Auto-compaction:
- Trigger: "If you don't set an auto-compact window, Claude Code compacts when the conversation reaches the model's context limit", with exceptions: Sonnet 4.6 and Opus 4.6 without extended context compact at the 200K boundary; models with a native 1M window compact "at about 967K tokens by default". No percentage threshold is documented.
- Setting: "auto-compact window" = "how full the context window can get before Claude Code compacts". Set via `/autocompact <value>` (per model, saved in `modelSettings`), `autoCompactWindow` in settings, `--autocompact <auto|tokens>` flag, `CLAUDE_CODE_AUTO_COMPACT_WINDOW` env (plain token count; overrides everything). Accepted: 100K to 1M tokens; `500k`, `1M`, `200` = 200,000. Capped at the model's context window.

Manual: `/compact [focus instructions]`; CLAUDE.md "Compact instructions" section is read by compaction. Compaction replaces history with a summary.
- Cache: compaction invalidates the conversation layer. The summary request reads the warm prefix from cache; after a long idle it reprocesses the full history uncached.

Hooks: `PreCompact` (matcher `manual|auto`; exit code 2 blocks compaction; top-level `decision` field). `PostCompact` (matcher `manual|auto`; exit 2 only shows stderr).

Learning the window used:
- Python SDK `get_context_usage()` returns `ContextUsageResponse` (breakdown by category, skill, tool; same data as `/context`). Costs token-counting API requests not in the message stream.
- Status line JSON carries `current_usage` and `prompt_cache` (statusline docs, not fetched directly).
- The stream `result.usage` is per-run totals, not a window size. A field for the window size was NOT found in the stream docs read.

### 6. Subagents and parallel work

Source: https://code.claude.com/docs/en/sub-agents

- Concurrency limit: default 20 running subagents; spawning the 21st via the Agent tool fails with `Concurrent subagent limit reached`. Change with `CLAUDE_CODE_MAX_CONCURRENT_SUBAGENTS` (positive int, v2.1.217+). Exempt under ultracode. Forks via `/subtask` take a slot and are never blocked. Resumes take a slot without checking the limit.
- Total spawn count per session: unlimited.
- Foreground: blocks main conversation; permission prompts pass through. Background: concurrent; prompts surface in main session naming the subagent. Background subagents get a smaller built-in tool set (except forks and resumed foreground ones). `Ctrl+B` backgrounds; `Ctrl+X Ctrl+K` stops all background subagents.
- Nesting: default depth 3 layers below main. `CLAUDE_CODE_MAX_SUBAGENT_SPAWN_DEPTH` (1 = no nesting). At limit the Agent tool is withheld except from forks.
- Context: each subagent has its own context window, fresh start (no parent history), own system prompt and tools. Not inherited: output style, auto memory, parent's window size. Fork inherits everything.
- Resume: by agent ID or name via `SendMessage` `to` field; keeps full history. Explore and Plan are one-shot.
- Cost: subagent requests count toward the same limits. Forks reuse the parent's cache.
- Stream: subagent messages carry `parent_tool_use_id`; `--forward-subagent-text` adds subagent text/thinking (v2.1.211+). Stream events not forwarded for subagents.

### 7. Gotchas for harness authors

Sources: headless, sessions, streaming-vs-single-mode, python SDK, hooks, prompt-caching.

- stdin: `-p` reads stdin; piped stdin capped at 10 MB (exceed = clear error, non-zero exit). If stdin is unreadable (e.g. parent closed its end) Claude prints a warning and continues with the CLI prompt. Before v2.1.211, unreadable stdin on Windows crashed or exited silently.
- Background work: after stdin closes, `-p` can keep running for background commands, subagents, Monitor watches, and self-paced `/loop` wakeups. Idle cap 10 min by default (`CLAUDE_CODE_PRINT_BG_WAIT_CEILING_MS`, 0 = no cap). Waiting prints `Waiting for background work to finish` on stderr (terminal only; with json/stream-json only when stdout is not a TTY).
- Stream drain: slow consumer delays exit, capped at 30 s (v2.1.214+).
- Windows: SIGTERM exit code 143; running Bash command process trees are terminated. No explicit Windows process-tree note besides stdin in the docs read.
- Hook timeouts: command/http/mcp_tool default 600 s; prompt 30 s; agent 60 s; 30 s for UserPromptSubmit/PreModelSwitch/PostModelSwitch; 10 s for MessageDisplay. SessionEnd hooks share a 1.5 s budget (a per-hook timeout raises it, up to 60 s). Hooks defined in project `.claude/settings.json` and `.mcp.json` run in `-p` even in untrusted folders unless `--bare`.
- `--bare`: skips hooks, skills, plugins, MCP (except `--mcp-config`), auto memory, CLAUDE.md; never reads OAuth/keychain (needs `ANTHROPIC_API_KEY` or `apiKeyHelper`). Documented as "recommended for scripted and SDK calls, and will become the default for -p in a future release."
- Python SDK streaming: a generator exception is logged at debug level and the session stalls with no raised error. TS: error text reads `Claude Code process aborted by user`.
- Python `receive_response()` ends at the first ResultMessage; a `session_state_changed` can come after it (use `receive_messages()`).
- `interrupt()` does not clear the buffer: drain before the next query.
- Working directory deleted mid-session: session continues; shell commands fail; warning message in stream-json.
- Transcript JSONL is internal and changes between versions. Do not parse it (sessions doc). Use `--resume`, `/export`, or the stream.
- `--system-prompt` / `--append-system-prompt` text is frozen at first request of a conversation (see 1).
- Subprocess per run: `claude -p` is a separate process per run; the CLI itself is not a long-lived daemon. (Agent view / `claude daemon` exists for background sessions; `claude daemon status`, `claude daemon run`.)
- Cached `--mcp-config` servers: `-p` waits up to `MCP_TIMEOUT` (30 s default) for pending servers before the first turn (v2.1.221+).
- Version drift: many flags gated by version in docs (e.g. `--permission-prompts` v2.1.259+, `--forward-subagent-text` v2.1.211+, `system/init capabilities` v2.1.205+, `--system-prompt-snapshot` v2.1.257+, `--json-schema` invalid schema exits v2.1.205+). Local binary is 2.1.296.
- Raw control protocol (stdin JSON control_request/control_response used by SDK): NOT documented in pages read. The public surface is the Agent SDK and `--input-format stream-json` user messages (format from streaming-vs-single-mode examples: `{"type":"user","message":{"role":"user","content":...},"parent_tool_use_id":null}`).

---------------------------------------------------------------------
## CODEX (codex-cli 0.162.1 local; app-server is experimental)

Doc redirects: developers.openai.com/codex/app-server, /agent-approvals-security, /config-advanced, /noninteractive, /cli/reference, /subagents all 308 to learn.chatgpt.com/docs/... (followed). `developers.openai.com/codex/noninteractive` also redirects; `learn.chatgpt.com/docs/subagents` returned 404, followed `agent-configuration/subagents` instead.

### 1. Session lifecycle

Sources:
- https://learn.chatgpt.com/docs/app-server (thread/start, thread/resume, thread/fork)
- https://github.com/openai/codex/blob/main/codex-rs/app-server/README.md
- https://learn.chatgpt.com/docs/developer-commands?surface=cli (codex resume / fork / exec resume)
- https://learn.chatgpt.com/docs/non-interactive-mode (codex exec resume)
- Local schema: ThreadStartParams/ThreadResumeParams/ThreadForkParams (`schema2/v2/`, combined JSON)

Facts:
- Protocol: `thread/start` creates a thread and emits `thread/started`. Optional `model, cwd, approvalPolicy, sandbox, personality, serviceName`. `historyMode: "paginated"` returns -32601 (not supported yet).
- `thread/resume` reopens a stored thread by `threadId`; later `turn/start` appends. Response shape = thread/start. "Resuming doesn't update updatedAt. Starting a turn does." If a required MCP server fails to init, resume fails. Model different from recorded: warning + one-time model-switch instruction. Also can resume "by history" or "by path" (ThreadResumeParams description); precedence history > non-empty path > thread_id for non-running threads.
- `thread/fork` creates a new thread id by copying history; emits `thread/started` with `forkedFromId`. `lastTurnId` copies through that turn (inclusive); in-progress `lastTurnId` is rejected; omitting it mid-turn records an interruption marker. `ephemeral: true` = in-memory, not listed.
- `thread.sessionId` is the root thread's id; forks keep the root's session id (ThreadStart description).
- `thread/list`: cursor-paginated, newest first by `createdAt`; `sourceKinds` default `cli`, `vscode`; filters `archived, isPinned, cwd, searchTerm`.
- Lifetime: "A thread with no subscribers unloads after a 30-minute grace period, emitting `thread/closed`." Persisted threads still resumable after unload (inferred from thread/resume).
- CLI: `codex resume` (picker), `codex resume --last`, `codex resume <SESSION_ID>` (UUID or name); `--all` (any dir); `--include-non-interactive`. `codex fork` (picker), `--last`, `<SESSION_ID>`. `codex exec resume [SESSION_ID] [PROMPT]` (`--last`, `--all`, `-` for stdin). `codex exec fork`: not in docs; GitHub issue #17568 proposes it (search result only, status unknown).
- Archive: `codex archive`, `codex unarchive`, `codex delete <SESSION>` (`--force` for UUID). Archive hides, delete removes.
- Where persisted: `history.persistence` = `save-all | none` (writes `history.jsonl`); `sqlite_home` = SQLite state DB for resumable state; `log_dir` default `$CODEX_HOME/log`. `CODEX_HOME` default: not stated. Retention: not stated.
- Resume + prompt cache: not documented (n/a).
- Known: `thread/resume` does not re-run in-flight items; `thread/rollback` deprecated.

### 2. Mid-session control

Sources:
- https://learn.chatgpt.com/docs/app-server (turn/start, turn/steer, turn/interrupt)
- https://github.com/openai/codex/blob/main/codex-rs/app-server/README.md
- Local schema: TurnStartParams, TurnSteerParams, TurnInterruptParams, CodexErrorInfo (`anyOf[5]`)

Interrupt:
- `turn/interrupt { threadId, turnId }` returns `{}`; the turn ends with `status: "interrupted"` (`turn/completed`).

Steer / queue:
- `turn/steer { threadId, expectedTurnId, input }`: appends user input to the in-flight turn, no new turn. `expectedTurnId` must match the active turn or the request fails. Fails if no turn is active (no `turn/started` emitted). Returns `{ turnId }`. Does NOT accept `model, cwd, sandboxPolicy, outputSchema` overrides.
- Steer rejected for turns that cannot take steering, e.g. `/review` or manual `/compact` (CodexErrorInfo description, local schema).
- `turn/start` during an active turn: schema says overrides are "Ignored when this request adds input to an active turn" (TurnStartParams `parentTurnId`, `rootTurnId`, `turnTrigger`, `serviceTierForTurn` descriptions), which means a turn/start while a turn is active adds input to that turn. The docs do not describe this in prose; treat as schema-derived.
- A queue exists in the protocol: notification `thread/queue/changed` (ThreadQueueChangedNotification `threadId`) and type `QueuedSubmission { clientUserMessageId, id, input }`. CLI `codex queue` ("Queue a message for an existing session") is described only in the help text. The app-server docs say "manual queue start" is one of the input RPCs gated by provider requirements. Exact queue/steer semantics not documented.
- `thread/inject_items` exists in the ClientRequest schema (not described in docs read).

Change model / effort / sandbox / approval mid-session:
- Any `turn/start` accepts overrides: `model, effort, summary, cwd, approvalPolicy, approvalsReviewer, sandboxPolicy, serviceTier, serviceTierForTurn, outputSchema`. Overrides "become defaults for later turns" (`outputSchema` only for the current turn). The docs do not state cache effects (n/a).
- `thread/settings/updated` notification exists (ThreadSettings: approvalPolicy, approvalsReviewer, collaborationMode, cwd, effort, model, sandboxPolicy, serviceTier, summary).
- Input RPCs (turn start, steer, review, compaction, manual queue start, active goal updates) are rejected when managed provider requirements no longer match the thread's provider; `turn/interrupt` stays allowed (app-server README).

### 3. Permission and sandbox modes; how a harness answers

Sources:
- https://learn.chatgpt.com/docs/agent-approvals-security
- https://learn.chatgpt.com/docs/config-file/config-reference
- https://learn.chatgpt.com/docs/developer-commands?surface=cli (flags)
- Local schema: AskForApproval, SandboxPolicy, CommandExecutionApprovalDecision, ServerRequest.json

Sandbox modes (`sandbox_mode` / `--sandbox` / `sandboxPolicy` type):
- `read-only`: "Codex can read files and run commands within the read-only sandbox." Actions outside may need approval.
- `workspace-write`: reads, edits and commands in the working directory automatically. Network off unless `network_access = true` under `[sandbox_workspace_write]`.
- `danger-full-access` (`--dangerously-bypass-approvals-and-sandbox`, alias `--yolo`): no sandbox, no approvals; "not recommended".
- Protected paths inside writable roots: `.git` read-only (also pointer file target), `.agents` and `.codex` read-only if directories (recursive).
- `codex exec`: default sandbox is read-only. `codex exec` requires a git repo unless `--skip-git-repo-check`.
- Launch defaults: version-controlled folder = `Auto` (workspace-write + on-request); non-VC folder = `read-only` (agent-approvals-security).
- Platforms: macOS Seatbelt via `sandbox-exec`; Linux `bwrap` + `seccomp`; Windows: MXC, with `elevated` and `unelevated` legacy fallbacks; `prefer_mxc = true` under `[features]`. Windows default mode: not stated. WSL1 unsupported from 0.115.
- `features.unified_exec` is on by default except on Windows.

Approval policies (`approval_policy` / `--ask-for-approval`):
- `on-request`: commands allowed by the sandbox run without approval; others ask.
- `never`: no prompts; failures returned to the model.
- `granular`: `{ granular = { sandbox_approval, rules, mcp_elicitations, request_permissions, skill_approval } }`; disabled categories are auto-rejected.
- `untrusted`: "no longer support[ed]"; can prevent the client from starting. `on-failure`: deprecated.
- `approvals_reviewer = user | auto_review`. Default `user`. `auto_review` sends eligible requests to a reviewer agent; adds model calls to usage (only under on-request or granular).

How a harness answers (server requests, JSON-RPC from server to client):
- `item/commandExecution/requestApproval`: decision values (local schema `CommandExecutionApprovalDecision`): `accept`, `acceptForSession`, `acceptWithExecpolicyAmendment` (`execpolicy_amendment: string[]`), `applyNetworkPolicyAmendment` (`network_policy_amendment`), `decline`, `cancel`.
- `item/fileChange/requestApproval`, `item/permissions/requestApproval` (`request_permissions` tool), `item/tool/requestUserInput`, `mcpServer/elicitation/request` (also `openai/userVerification`), `item/tool/call` (dynamic tools), `account/chatgptAuthTokens/refresh`, `attestation/generate`. Legacy v1 requests in schema: `applyPatchApproval`, `execCommandApproval`.
- Notification `serverRequest/resolved` closes a request. Notification `item/autoApprovalReview/started|completed` when auto_review.
- Non-interactive flows: "an action that needs new approval fails and Codex surfaces the error back to the parent workflow" (subagents doc).
- `codex exec` approval default: not stated on the page read.
- Hooks: `hook/started`, `hook/completed` notifications exist; `features.hooks` default not stated.

### 4. Streaming events

Sources:
- https://learn.chatgpt.com/docs/app-server (notifications)
- https://github.com/openai/codex/blob/main/codex-rs/app-server/README.md
- https://learn.chatgpt.com/docs/non-interactive-mode (`codex exec --json`)
- Local schema: ServerNotification.json, TokenUsageBreakdown, ThreadTokenUsage, TurnError, CodexErrorInfo, RateLimitSnapshot

Notifications (app-server, JSON-RPC, no `id`):
- Turn: `turn/started` (`items: []`, `status: "inProgress"`), `turn/completed` (`status`: `completed | interrupted | failed | inProgress` enum in schema; `error.message`, `error.codexErrorInfo`, `error.additionalDetails`), `turn/diff/updated` and `turn/plan/updated` (`items` empty; do not use for items), `turn/moderationMetadata`.
- Item: `item/started`, `item/completed` (ThreadItem types: `userMessage, hookPrompt, agentMessage, functionCallOutput, plan, reasoning, commandExecution, fileChange, mcpToolCall, dynamicToolCall, collabAgentToolCall, subAgentActivity, webSearch, imageView, sleep, imageGeneration, enteredReviewMode, exitedReviewMode, contextCompaction`), `item/agentMessage/delta`, `item/plan/delta`, `item/reasoning/summaryTextDelta|summaryPartAdded|textDelta`, `item/commandExecution/outputDelta`, `item/commandExecution/terminalInteraction`, `item/fileChange/outputDelta|patchUpdated`, `item/mcpToolCall/progress`. README: "Use item/* notifications as the source of truth."
- Tokens: `thread/tokenUsage/updated` `{ threadId, turnId, tokenUsage: { last: TokenUsageBreakdown, total: TokenUsageBreakdown, modelContextWindow } }`. `TokenUsageBreakdown` fields: `inputTokens, cachedInputTokens, cacheWriteInputTokens (default 0), outputTokens, reasoningOutputTokens, totalTokens`. `modelContextWindow` is the window the harness can learn from (nullable).
- Rate limits: `account/rateLimits/updated` (sparse; merge into last `account/rateLimits/read` result). `RateLimitSnapshot`: `primary`/`secondary` windows `{usedPercent (int), windowDurationMins, resetsAt}`, `credits`, `planType`, `rateLimitReachedType` (`rate_limit_reached | workspace_owner_credits_depleted | workspace_member_credits_depleted | workspace_owner_usage_limit_reached | workspace_member_usage_limit_reached`), `spendControlReached`.
- Errors: `error` notification `{ threadId, turnId, willRetry, error: TurnError }`. `CodexErrorInfo` values: `contextWindowExceeded, sessionBudgetExceeded, usageLimitExceeded, rateLimitExceeded, flexUnavailable, serverOverloaded, cyberPolicy, misalignmentPolicyViolation, tooManyDenials, internalServerError, unauthorized, badRequest, threadRollbackFailed, sandboxError, other`, plus object variants `httpConnectionFailed {httpStatusCode}`, `responseStreamConnectionFailed`, and one more object variant for SSE stream disconnection (not read in full). WebSocket overload: JSON-RPC -32001 "Server overloaded; retry later." (websocket only).
- Compaction: item type `contextCompaction` (ThreadItem); deprecated `thread/compacted` notification (`ContextCompactedNotification`: "Deprecated: Use ContextCompaction item type instead.").
- Threads: `thread/status/changed` (`ThreadStatus`: `notLoaded | idle | systemError | active` with `activeFlags: waitingOnApproval | waitingOnUserInput`), `thread/closed`, `thread/archived`, `thread/deleted`, `thread/goal/updated`, `thread/settings/updated`, `thread/name/updated`, `thread/queue/changed`, `thread/attachment/updated`, `model/rerouted`, `model/verification`, `model/safetyBuffering/updated`, `process/exited`, `process/outputDelta`, `fs/changed`, `skills/changed`, `warning`, `configWarning`, `deprecationNotice`, `error`.
- Realtime (thread/realtime/*) and `remoteControl/status/changed` also exist (not read).

`codex exec --json` (JSONL to stdout, one event per state change):
- Event types: `thread.started`, `turn.started`, `turn.completed` (includes `usage`), `turn.failed`, `item.*` (items: agent message, reasoning, command execution, file change, MCP tool call, web search, plan update), `error`.
- Without `--json`: progress on stderr, only final message on stdout. `-o/--output-last-message <path>`. `--output-schema <file>`.
- Exit codes: not documented. A required MCP server (`required = true`) that fails to start stops the run with an error.

### 5. Context management

Sources:
- https://learn.chatgpt.com/docs/config-file/config-reference (keys)
- https://learn.chatgpt.com/docs/config-file/config-advanced (partial; keys only)
- https://learn.chatgpt.com/docs/app-server (`thread/compact/start`, input RPC gating)
- Local schema: AutoCompactTokenLimitScope, ThreadCompactStartParams, ContextCompaction item

- Config keys (config-reference): `model_context_window` (number; "Context window tokens for the active model"; example 128000), `model_auto_compact_token_limit` (number; "unset uses model defaults"), `model_auto_compact_token_limit_scope` = `total | body_after_prefix` (default `total`; `body_after_prefix` counts growth after the carried window prefix), `compact_prompt`, `experimental_compact_prompt_file`.
- Official default threshold value: NOT stated. (A search summary of third-party blogs claimed a 90%-of-window ceiling and a 0.95 threshold in an old issue; unverified, not used.)
- Manual compaction: `thread/compact/start { threadId }` RPC; CLI `/compact` (interactive only); compaction rejected while `/review` or manual `/compact` running (steer rejected).
- Compaction runs as a turn item (`contextCompaction`); usage counted in `task.compact` metric (remote or local).
- Learn the window: `thread/tokenUsage/updated.tokenUsage.modelContextWindow` (nullable) plus `last`/`total` breakdown. `turn.token_usage` metric histogram exists (OpenTelemetry).
- `CodexErrorInfo.contextWindowExceeded` for overflow.

### 6. Subagents and parallel work

Sources:
- https://learn.chatgpt.com/docs/agent-configuration/subagents
- https://learn.chatgpt.com/docs/app-server (one process, many threads)
- `codex app-server daemon --help`, `codex agents --help` (local)

- Spawn: "ChatGPT or Codex handles orchestration across agents, including spawning new subagents, routing follow-up instructions, waiting for results, and closing agent threads." Custom agents: TOML in `~/.codex/agents/` or `.codex/agents/` with `name`, `description`, `developer_instructions`.
- Parallel: "Run specialized subagents in parallel for exploration, tests, or log analysis." Concurrency cap: `agents.max_concurrent_threads_per_session` (cap on concurrently open spawned threads, excluding primary); unset = "Codex chooses the default" (number NOT stated). `agents.max_threads` legacy alias.
- Depth limit: absent.
- Keys: `agents.enabled`, `agents.default_subagent_model`, `agents.default_subagent_reasoning_effort`, `agents.interrupt_message`. Feature `features.multi_agent` on by default (config-reference).
- Sandbox/approvals: "Subagents inherit your current sandbox policy." Parent's live runtime overrides re-applied on spawn. Custom agents can override with `sandbox_mode`. Non-interactive: an action needing new approval fails and the error goes back to the parent.
- Context: each subagent has its own agent thread. Explicit isolation guarantee: not stated (the docs advise returning summaries).
- Results: "The main thread collects the subagent results into its final response."
- Model: inherits parent model and reasoning effort unless configured.
- App-server: many threads per process (one `initialize` per connection). `codex agents` browses "all agent sessions on the shared local app-server daemon". `codex app-server daemon start|restart|stop|version|bootstrap`. `--no-daemon` on `codex`/`exec` runs without the shared daemon.

### 7. Gotchas for harness authors

Sources: app-server README, app-server docs, non-interactive mode, config reference, local `--help`.

- Protocol is experimental: `codex app-server` "[experimental] ... may change without notice" (developer-commands). Pin the codex version and regenerate schema per release (`codex app-server generate-json-schema`).
- Handshake: send `initialize` once per connection, then `initialized` notification. Earlier requests get `Not initialized`; repeat `initialize` gets `Already initialized`. Experimental methods/fields require `initialize.capabilities.experimentalApi: true` or the server rejects them with "requires experimentalApi capability".
- Transport: stdio JSONL is the default (`--listen stdio://`; `--stdio` alias). WebSocket is "experimental and unsupported"; non-loopback WebSocket allows unauthenticated connections by default during rollout. Unix socket uses WebSocket over HTTP Upgrade.
- Shared process: one app-server serves many threads; attachment create/delete for a thread serialized across connections; read-state delivery stops on `thread/unsubscribe` or disconnect. A thread with no subscribers unloads after 30 min (emits `thread/closed`). Closing a connection cancels its gateway OAuth login.
- `thread/shellCommand` and `process/*` run outside the sandbox; expose only for explicit user actions.
- `thread/resume` fails if a required MCP server fails to initialize.
- Windows: `unified_exec` off by default on Windows; sandbox modes MXC / elevated / unelevated; no Windows default sandbox stated. Process-tree / zombie handling: not documented (n/a).
- stdin: `codex exec` reads prompt from stdin when no arg or `-`; piped stdin + arg = stdin appended as context. Exit codes: not documented. Non-interactive default sandbox read-only; `danger-full-access` only in isolated runners.
- `codex exec` on a non-git directory fails unless `--skip-git-repo-check`.
- `--full-auto` deprecated in exec; use `--sandbox workspace-write`.
- `CODEX_API_KEY` / `OPENAI_API_KEY` in job env is warned against for repo-controlled code.
- `approval_policy = "untrusted"` unsupported; `on-failure` deprecated.
- Process exit / EOF on stdin behavior of app-server: not documented (n/a).
- `thread/rollback` deprecated; use `thread/fork` with `lastTurnId`.
- Overload: -32001 over websocket; retry with backoff and jitter.
- `turn/start` on a provider mismatch is rejected (managed provider requirements).
- Feature defaults: `features.goals`, `features.multi_agent`, `features.fast_mode`, `features.shell_snapshot` on; `features.memories`, `features.code_mode.enabled`, `features.prevent_idle_sleep`, `features.network_proxy.enabled` off (config-reference).

---------------------------------------------------------------------
## Unknown or unclear

Claude Code:
- Raw stream-json control protocol (control_request / control_response for permissions and interrupt): not in public docs read; SDK wraps it. Only public: `--input-format stream-json` user messages.
- `query()` while a turn is running (Agent SDK): not documented.
- Whether `/model` keeps conversation history: not stated.
- Exact auto-compact trigger percentage: not stated (only model-context-limit and 200K / ~967K boundaries).
- Context window size exposed in the stream: no field found in docs read (only `get_context_usage()` and statusline JSON).
- Whether `PermissionRequest` hook fires in `-p` mode: not stated in the page read (hooks page truncated after 100k chars).
- Sandbox page not read in full.

Codex:
- Official default for `model_auto_compact_token_limit` (numeric): not stated.
- Session storage location under `CODEX_HOME` and retention: not stated in docs read.
- `CODEX_HOME` default: not stated.
- `codex exec fork`: not in docs (GitHub issue #17568 proposes it; status unknown).
- `codex queue` command and `thread/queue/changed` semantics: not documented.
- Whether `turn/start` during an active turn queues or steers: schema hints it adds input to the active turn; not stated in prose.
- Cache behavior of Codex on resume and on overrides (model, effort, sandbox): not documented.
- Exit codes of `codex exec`: not documented.
- Codex subagent max concurrency numeric default and depth limit: not stated.
- Windows default sandbox mode: not stated.
- App-server stdin EOF / process exit / orphan behavior: not documented.
- Approval default for `codex exec`: not stated.
- Subagents docs read only the "agent-configuration/subagents" page; `learn.chatgpt.com/docs/subagents` returned 404.

---------------------------------------------------------------------
## Questions worth asking

1. Pin Claude Code to a minimum version (features gated at 2.1.205 to 2.1.296) and refuse older binaries, or support a range?
2. Use `--bare` for Claude runs (documented as recommended for scripted calls, but it drops the user's CLAUDE.md, hooks and MCP and needs API-key auth, not subscription login), or keep normal mode and accept project hooks and `.mcp.json` running in `-p`?
3. Accept `codex app-server` as the protocol even though it is labeled experimental and "may change without notice", with a generated-schema check per Codex release?
4. For a message sent while a turn runs: queue until the tool calls finish (Claude TUI behavior), steer the active turn (Codex `turn/steer` with `expectedTurnId`), or interrupt first? Claude's SDK `query()`-during-turn is undocumented, so this needs a measured test.
5. Model and effort switches mid-session cost a cache miss on Claude (and on Codex, undocumented). Allow them mid-session with a warning, or restrict them to the next conversation?
6. Faber keeps its own JSON conversations, so should Claude runs use `--no-session-persistence` and replay context from Faber, or resume Claude's internal transcript by ID? (Claude's transcript format is internal and changes between versions.)
7. Default Codex approval policy: `on-request` with `workspace-write` (approvals to the user through `item/commandExecution/requestApproval` with the decision list accept / acceptForSession / decline / cancel), or `approvals_reviewer = auto_review`?
8. RSS budget: each `claude -p` run is its own process, while Codex is one shared app-server with a 30-minute idle unload. Measure both against the 250 MB end-to-end ceiling before choosing keep-alive policy?

# R4: Claude Code and Codex CLI, changes since mid-July 2026

Date seen: 2026-10-11 (all sources read that day). Window: mid-July to 2026-10-11.

## Method and caveats

- Claude Code: raw `CHANGELOG.md` from `raw.githubusercontent.com/anthropics/claude-code/main` (downloaded 2026-10-11; copy at `r4/cc_CHANGELOG.md`). Version dates come from the GitHub releases API (`published_at`), because the raw changelog headers have no dates. Window extract: `r4/cc_window_2.1.208-2.1.296.md` (versions 2.1.208 on 2026-07-14 through 2.1.296 on 2026-10-09).
- Codex: GitHub releases API (`gh api`), bodies fetched per tag (`r4/codex_stable_bodies.md`). Index of all releases: `r4/codex_releases_index.tsv`.
- Docs pages used: code.claude.com/docs (model-config, setup, env vars), platform.claude.com (model deprecations, Models list), developers.openai.com/api/docs/deprecations (redirected from platform.openai.com), learn.chatgpt.com/docs (redirected from developers.openai.com/codex; changelog, app-server, developer-commands).
- WebFetch summaries of the Claude Code changelog and the Codex changelog page were partly model-written and contradicted each other in places (e.g. which version added the PreToolUse fail-closed change). Items below marked "raw" were checked in the raw changelog text. Items marked "WebFetch" were not checked against raw text.
- GitHub issue search: `gh search` returns comment counts, not reaction counts. The "most upvoted" ordering below is by comment count (c=N), and the reactions sort returned unordered data.

## Claude Code

### Release cadence and numbering
- Tags `v2.1.X` (patch-only numbering, no semver minor/major bumps in the window). 86 stable releases from v2.1.198 (2026-07-01) to v2.1.296 (2026-10-09), about one per day. No prerelease tags in the window (raw: releases API `prerelease=false`).
- Pin/detect: `claude --version` prints the version. Native installer accepts a version or channel: `install.sh | bash -s 2.1.89`, `bash -s stable` (setup docs, WebFetch of setup page, verified text).
- Auto-update controls (setup page and env-vars, WebFetch of setup page):
  - `autoUpdatesChannel`: `"latest"` (default) or `"stable"` (about one week old).
  - `minimumVersion` (settings) is a floor for background updates and `claude update`.
  - Managed settings `requiredMinimumVersion` and `requiredMaximumVersion` make the CLI refuse to start outside a range.
  - `DISABLE_AUTOUPDATER=1` (env) stops only the background check; `claude update` and `claude install` still work.
  - `DISABLE_UPDATES` (env) blocks all update paths. Added in v2.1.118 (raw line 5567).
  - Homebrew, WinGet, apt, dnf and apk do not auto-update by default. `CLAUDE_CODE_PACKAGE_MANAGER_AUTO_UPDATE=1` opts Homebrew and WinGet in.

### 1. Breaking changes, renamed or removed flags, changed defaults (raw unless noted)

Defaults:
- v2.1.296 (2026-10-09): default MCP tool-description limit raised 2,048 to 4,096 chars (`CLAUDE_CODE_MAX_MCP_DESCRIPTION_LENGTH` controls it). `←` now stops an in-flight turn.
- v2.1.295 (2026-10-08): `onFailure: "block"` for command and HTTP hooks (new). WebSocket MCP messages over 16 MiB close the connection. claude.ai connectors negotiate MCP protocol 2026-07-28 by default (`MCP_PROTOCOL_NEGOTIATION=legacy` opts out).
- v2.1.293 (2026-10-07): Claude Haiku 5.5 (`claude-haiku-5-5`) becomes default Haiku on the Anthropic API.
- v2.1.292 (2026-10-06): local stdio MCP connections negotiate protocol 2026-07-28 by default on every install (opt out `MCP_PROTOCOL_NEGOTIATION=legacy`).
- v2.1.288 (2026-10-02): background command time limit applies only to unattended sessions (`-p`, SDK, CI, cloud). Auto-mode classifier ignores an `ANTHROPIC_DEFAULT_SONNET_MODEL` pin naming Sonnet 5.5 or Opus 5.5 and uses Sonnet 5.
- v2.1.287 (2026-10-01): Opus 4.7+ and Fable use the 1M context window by default on Bedrock, Vertex, Foundry and the Claude apps gateway, with no `[1m]` suffix (`CLAUDE_CODE_DISABLE_1M_CONTEXT=1` restores 200K). Automatic model switches after a flagged message keep the current effort level. `alwaysLoad: false` defers all of a server's tools (raw line ~788-794 region of the window).
- v2.1.285 (2026-09-29): `-p` and the Python Agent SDK start in auto mode when no permission mode is configured on third-party providers or with telemetry off. Background Bash/PowerShell commands stop after `timeout` (default 30 min, max 2 h). Sessions behind a custom `ANTHROPIC_BASE_URL` use the 1M window for Opus 4.7+, Sonnet 5+, Fable.
- v2.1.284 (2026-09-28): interactive terminal and VS Code sessions start in auto mode when no permission mode is configured, on every plan and provider (raw line 1110). `permissions.defaultMode` overrides. Claude Sonnet 5.5 (`claude-sonnet-5-5`) becomes default Sonnet on the Anthropic API. Ultracode is a separate `/effort` toggle and no longer forces xhigh.
- v2.1.283 (2026-09-25): interactive sessions on third-party providers or with telemetry off start in auto mode (raw line 1214).
- v2.1.282 (2026-09-24): server-side auto-mode classifier is default on a direct Anthropic API connection when telemetry is off (`CLAUDE_CODE_AUTO_MODE_SERVER=0` opts out).
- v2.1.280 (2026-09-22): Pro and Team Standard default model changes from Sonnet to Opus. `claude-opus-5-5` becomes default Opus. Saved effort from before per-model `/effort` no longer applies to new models. `PermissionRequest` hooks no longer run `agent`-type hooks (they error).
- v2.1.278 (2026-09-19): server-side auto-mode classifier default for Claude API and Enterprise users, Bedrock, Vertex, Foundry and gateways.
- v2.1.257 (2026-09-01): `claude-fable-5-1` becomes default Fable model. `defaultMode: "bypassPermissions"` in project `.claude/settings.json` and `settings.local.json` is ignored (raw line 2706 region).
- v2.1.248 (2026-08-27): `--restricted` / `CLAUDE_CODE_RESTRICTED=1` flag (new; removes command tools unless named in `--tools`).
- v2.1.243 (2026-08-25): `promptCacheTtl` and `subagentPromptCacheTtl` settings (1-hour cache for main conversation while subagents stay 5 min). Agent frontmatter `experimental.cacheTtl` added in 2.1.248.
- v2.1.233 (2026-08-14): task tools (TaskCreate/Get/Update/List, TodoWrite) unavailable on Opus 4.8, Sonnet 5, Fable 5, Mythos 5 and newer. `CLAUDE_CODE_ENABLE_TODO_TOOLS=1` restores them (raw line 3275).
- v2.1.232 (2026-08-13): subagent forking on by default; `subagent_type: "fork"` inherits full conversation and prompt cache. Non-teammate agent spawns in interactive sessions run in background by default.
- v2.1.268 (2026-09-10): TaskCreate/Get/Update/List and TodoWrite offered only on Claude 3.x, Opus 4.0-4.7, Sonnet 4.0-4.6, Haiku 4.5 (raw line 2276).
- v2.1.251 (2026-08-28): `CLAUDE_CODE_SUBAGENT_MODEL` sets the default subagent model (agent `model:` and per-spawn model override it). Enterprise seat default model is Opus 5. `/effort` saves default per model. Project `.claude/settings.json` `env` no longer sets `CLAUDE_CONFIG_DIR`, `CLAUDE_CODE_TMPDIR`, `TMPDIR`/`TMP`/`TEMP`.
- v2.1.236 (2026-08-19): `ANTHROPIC_DEFAULT_MODEL` sets start model (`/model` pick overrides it).

Renamed or removed:
- v2.1.288: `claude project purge` renamed to `claude purge`; old name works and prints a notice (raw line 694).
- v2.1.277 (2026-09-18): deprecated TaskOutput tool removed; `taskOutputMaxChars` and `TASK_MAX_OUTPUT_LENGTH` no effect. Background Haiku auto-title request removed from `claude -p` runs outside SDK/IDE.
- v2.1.274 (2026-09-17): `"type": "sdk"` MCP entries in `.mcp.json`, settings, plugins, agent files are skipped with a warning (raw line 1894). MCP v2 client and MCP 2026-07-28 negotiation default on Bedrock, Vertex, Foundry and telemetry-disabled installs (raw line 1892).
- v2.1.271 (2026-09-14): `Monitor` watches always have a deadline (max 30 min; 10 in single-prompt `-p` runs); replaces the no-timeout `persistent` option (raw line 2079).
- v2.1.261 (2026-09-04): `forceLoginMethod: "gateway"` ignores leftover API keys or claude.ai logins; `keybindingFlavor` has no effect (WebFetch).
- v2.1.259 (2026-09-02): `allowedMcpServers` governs only user-added servers; a literal `managed-mcp.json` server now loads (use `deniedMcpServers` to block) (WebFetch).
- v2.1.234 (2026-08-17): "Default teammate model" setting removed from `/config` (raw line 3250).
- v2.1.222 (2026-08-04): ultraplan feature removed (raw line 3498).
- v2.1.224 (2026-08-07): 200-subagent-per-session cap removed (raw line 3444).
- v2.1.281: `(removed)` `/agents` menu entry removed (raw line 1488).
- v2.1.286 (2026-09-30): one retry limit covers a whole model call; with default settings a failing call sends at most 14 requests (raw line 872). `--bare` now connects only MCP servers named on the command line, sends no system reminders, starts no background tasks (raw line 873).

Harness-relevant behaviour changes:
- v2.1.288 (raw line 674): PreToolUse and PermissionRequest hooks are blocked, not skipped, when matching fails or input cannot be serialized (WebFetch of the 2.1.287 entry attributed this to 2.1.287/2.1.292; the raw text places it in 2.1.288).
- v2.1.281 (2026-09-23): `ctrl+enter` (send now) moves running tools to background instead of cancelling the turn. Self-hosted runner wrappers must use `--system-prompt-file` or `--append-system-prompt-file`.
- v2.1.275 (2026-09-17): `ctrl+enter` interrupts current turn and sends queued messages.
- v2.1.260 (2026-09-03): `--kill-session-after-min` releases waiting sessions instead of killing them.

### 2. New features useful to a harness (raw unless noted)

Stream and headless output:
- `system/init` stream-json: `plugin_errors` with `path` (v2.1.283, raw line 1158).
- `--output-format stream-json` and SDK: `result.usage` on non-streaming fallback fixed (v2.1.283).
- `-p` text output prints each turn's response when turn ends (v2.1.295 fix, raw line 101).
- `--include-partial-messages` SDK hosts: `ping` stream event every 30 s on Anthropic API and gateways during non-streaming fallback (v2.1.285, raw line 988).
- `--forward-subagent-text` stream-json now includes forked-skill subagent messages (v2.1.273 area, raw line 1740).
- Headless `rate_limit_event` warnings now say whether extra usage is on (v2.1.295, raw line 190).
- Status line JSON: `rate_limits.spend_limit` (gains `used_usd`, `limit_usd`, `period` in 2.1.284), `prompt_cache` object (v2.1.251, WebFetch), `rate_limits.five_hour.used_percentage` (referenced in issue #75408).
- Hooks: `PreModelSwitch` and `PostModelSwitch` events (v2.1.251, WebFetch). `SessionStart` resume hooks get staleness and re-cache cost.
- `onFailure: "block"` for command and HTTP hooks (v2.1.295).

Resume, fork, sessions:
- `claude --resume <id> --bg` continues under the session's own ID (WebFetch, v2.1.252 area).
- `claude attach`, `logs`, `stop`, `respawn`, `rm` (listed in `--help` v2.1.251); partial names accepted (v2.1.290).
- `CLAUDE_CODE_RESUME_INTERRUPTED_TURN` ignores turns older than 6 h (v2.1.269); `..._MAX_AGE_MS` override.
- Plan mode restored on `--continue`, `--resume <id>` and resume picker (v2.1.292/2.1.290 fixes).
- `claude -p --resume <file>` uses a fresh session ID when transcript's ID is malformed (v2.1.261, WebFetch).
- Fork subagents run under parent's permission mode (v2.1.285).

Model, effort, cache:
- `CLAUDE_CODE_SUBAGENT_MODEL_FORCE` (v2.1.257 WebFetch).
- `set_model` (SDK `setModel`) applies new model's output-token limit and auto-compact window (v2.1.285).
- `modelPicker` (settings, curates `/model`) and `modelPricing` (managed) (v2.1.243).
- `CLAUDE_CODE_OVERLOADED_RETRY_BASE_DELAY_MS` and `_MAX_DELAY_MS` (v2.1.292, v2.1.296); `CLAUDE_CODE_RETRY_WATCHDOG_MAX_WAIT_MS` (v2.1.295).
- `CLAUDE_CODE_NONSTREAMING_TIMEOUT_RETRIES` (v2.1.285). `CLAUDE_CODE_DISABLE_STRUCTURED_OUTPUTS` (v2.1.288).
- OTel: `effort` attribute on `claude_code.llm_request` span (v2.1.274); `OTEL_LOG_TOOL_CONTENT=1` (v2.1.283).

Quota and account:
- `/usage` shows model-specific weekly limit rows (v2.1.261). `/rate-limit-options` (v2.1.284). Spend-limit dollar amounts for Claude apps gateway in `/usage` and status line (v2.1.284).
- `claude auth status` reports `api_key` for Console sign-in (v2.1.286). `configDirectory` in `claude auth status --json` (v2.1.268).
- `claude plugin install|uninstall|update|enable|disable --json` (v2.1.268); `claude plugin validate --json` (v2.1.259); `claude plugin eval` (v2.1.269).
- Model listing: no programmatic Claude Code listing documented. Gateway model discovery via `CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY=1` (uses gateway `/v1/models`, WebFetch model-config page).

Permissions and sandbox for unattended runs:
- `--permission-prompts none` for unattended hosts (denies anything that would prompt) (v2.1.259, WebFetch).
- `-p --permission-prompt-tool` routes background subagent permission requests to the prompt tool (v2.1.285 fix, raw).
- `--tools` and `--restricted` apply to built-in tools registered after launch (v2.1.295 fix).

### 3. Model lifecycle (Claude)

Source: platform.claude.com model-deprecations page (WebFetch, verified text), models-list API page (WebFetch, verified text), model-config page (WebFetch summary).

Current models and status (deprecation page table, 2026-10-11):
- Active: `claude-fable-5-1` (not sooner than 2027-09-01), `claude-mythos-5-1` (2027-09-01), `claude-fable-5` (2027-06-09), `claude-mythos-5` (2027-06-09), `claude-opus-5-5` (2027-09-22), `claude-opus-5` (2027-07-24), `claude-opus-4-8` (2027-05-28), `claude-opus-4-7` (2027-04-16), `claude-opus-4-6` (2027-02-05), `claude-opus-4-5-20251101` (not sooner than 2026-11-24), `claude-sonnet-5-5` (2027-09-28), `claude-sonnet-5` (2027-06-30), `claude-sonnet-4-6` (2027-02-17), `claude-haiku-5-5` (2027-10-07), `claude-haiku-4-5-20251001` (not sooner than 2026-10-15).
- Deprecated: `claude-sonnet-4-5-20250929` (deprecated 2026-09-30, retires 2026-11-30, replacement `claude-sonnet-5-5`). `claude-mythos-preview` (deprecated 2026-06-09, retirement TBA).
- Retired: `claude-opus-4-1-20250805` (retired 2026-08-05, replacement `claude-opus-4-8`), `claude-opus-4-20250514` and `claude-sonnet-4-20250514` (retired 2026-06-15).
- Notification: at least 60 days' notice for publicly released models (deprecation page).
- Parameter deprecation: `temperature`, `top_p`, `top_k` return 400 on non-default values for Claude 4.7+ models.

Aliases (model-config page, WebFetch summary; per-provider values as listed there):
- `default`: clears override and uses account runtime default. Opus 5.5 on Pro, Max, Team, Enterprise, Anthropic API, Claude Platform on AWS, Bedrock, Agent Platform. Sonnet 4.5 on Microsoft Foundry.
- `best`: same as `fable` where Fable is available, otherwise same as `opus`.
- `fable`: Fable 5.1 (unless `ANTHROPIC_DEFAULT_FABLE_MODEL` set). Fable 5 in Claude apps gateway sessions.
- `opus`: Opus 5.5 (Anthropic API, Claude Platform on AWS, Bedrock, Agent Platform); Opus 4.6 on Microsoft Foundry.
- `sonnet`: Sonnet 5.5 (Anthropic API); Sonnet 4.6 (Claude Platform on AWS); Sonnet 4.5 (Bedrock, Agent Platform, Foundry).
- `haiku`: Haiku 5.5 (Anthropic API); Haiku 4.5 (Claude Platform on AWS, Bedrock, Agent Platform, Foundry).
- `opusplan`: `opus` in plan mode, `sonnet` for execution. `sonnet[1m]`, `opus[1m]`: 1M context.
- Version floors from the page: Opus 5.5 needs v2.1.280+, Sonnet 5.5 v2.1.284+, Haiku 5.5 v2.1.293+, Fable 5.1 v2.1.257+.

Env vars (model-config page): `ANTHROPIC_MODEL` (launch model, below `--model`, above `model` setting, overrides a `/model` pick); `ANTHROPIC_DEFAULT_MODEL` (v2.1.236+, start model, ignored if `default`, `inherit`, `opusplan`, `haiku`); `ANTHROPIC_DEFAULT_OPUS_MODEL`, `ANTHROPIC_DEFAULT_SONNET_MODEL`, `ANTHROPIC_DEFAULT_HAIKU_MODEL`, `ANTHROPIC_DEFAULT_FABLE_MODEL`; `CLAUDE_CODE_SUBAGENT_MODEL` (table cut off in the fetch).

Default effort (model-config page, WebFetch): explicit (`CLAUDE_CODE_EFFORT_LEVEL`, `--effort`, `/effort`) > settings (`modelSettings` per model, or `effortLevel` with `low`, `medium`, `high`, `xhigh`, not `max`) > model default. Most models default to `high`. Opus 5.5, Sonnet 5.5 and Haiku 5.5 default to `medium`. Opus 4.7 defaults to `xhigh`.

How new models appear: model id added to CLI (version floor noted in changelog, e.g. v2.1.280 Opus 5.5, v2.1.284 Sonnet 5.5, v2.1.293 Haiku 5.5, v2.1.257 Fable 5.1). Unknown model ids print `[claude-code:unrecognized_model]` to stderr in print mode (v2.1.233).

Listing programmatically (Claude equivalent of Codex `model/list`):
- Anthropic API: `GET https://api.anthropic.com/v1/models`, headers `anthropic-version` and `X-Api-Key`. Query: `limit` (1-1000, default 20), `before_id`, `after_id`, `lifecycle` (`active`, `deprecated`, `retired`, up to 3). Default lifecycle filter returns active and deprecated only. Response: `data[]` with `id`, `display_name`, `lifecycle`, `created_at`, `deprecated_at`, `retires_at`, `max_input_tokens`, `max_tokens`, `capabilities` (incl. `effort` levels, `thinking`, `server_tools`, `context_management`), plus `has_more`, `first_id`, `last_id`. The page's example contains placeholder values; treat them as shape only.
- Claude Code CLI: no documented way to list models headlessly. Not verified whether the subscription (OAuth) login can call `/v1/models`.

### 4. Release and quota-related issues, Claude Code (open at search time, 2026-10-11)

Ranked by comment count within each search (gh does not expose reactions):

Headless hangs and startup hangs:
- #89272 Headless `claude -p` deadlocks in uninterruptible `openat` on macOS; interactive unaffected. Opened 2026-08-24.
- #78903 `claude -p` hangs before first token when started from a macOS launchd job; main thread in `kevent64` waiting on `/bin/zsh -c -l` shell-snapshot child; `SHELL=/bin/sh` works around it. Opened 2026-07-18, c1.
- #100241 macOS startup hangs forever (`openat`) when `known_marketplaces.json` `installLocation` is under `/home` (autofs). Opened 2026-10-07.
- #99653 Start-up hangs forever (main thread in `open()`) when a permission rule path resolves to a TCC-protected location. Opened 2026-10-05.
- #26224 "[URGENT] Claude Code is hanging/freezing/stuck... 5-20 minutes". Opened 2026-02-17, c131. Mode (headless or interactive) not stated in the search result.
- #91140 `claude -p` with agent teams: parked unnamed subagents get no second turn after their own background Bash or a fresh child completes. Opened 2026-09-01, c1.
- #98343 Auto-compact hangs indefinitely on Bedrock: post-compact `CountTokens` call has no timeout and cannot be interrupted. Opened 2026-09-30.

Stream and connection stalls:
- #100353 Since 2.1.213, one connection error disables HTTP keep-alive for the rest of the process; long sessions degrade into stream-idle-watchdog stalls and full-conversation resends until restart. Opened 2026-10-08.
- #33949 SSE streaming hangs indefinitely (no timeout); ESC cannot fully cancel. Opened 2026-03-13, c40.
- #46987 `API Error: Stream idle timeout - partial response received`. Opened 2026-04-12, c184.
- #88900 Streaming responses stall after ~0.5-2.7 KB, triggering 180 s byte-watchdog. Opened 2026-08-22.
- #93155 Requests silently stall 900 s with no error or retry logged. Opened 2026-09-09, c2.
- #91502 Established connections go silent; client waits 180 s+ before retry (retry then succeeds in ~1.5 s). Opened 2026-09-02.
- #94225 ECONNRESET on direct ISP path: Anthropic ingress resets TLS 1.3 handshakes carrying X25519MLKEM768; classical key share or VPN works (re-open of #74767; still on 2.1.270). Opened 2026-09-14, c8.
- #96153 ECONNRESET with `claude-fable-5-1` at normal effort; works at MAX. Opened 2026-09-22.
- #91982 Cross-session message delivered to a stream-json session emits no user frame on stdout; SDK hosts render a reply with no prompt. Opened 2026-09-04. Directly relevant to stream-json consumers.
- #89027 VS Code extension: `content_block_delta` and `message_stop` without a current message (regression after 2.1.235). Opened 2026-08-23.
- #93549 Remote-control routine sessions are never sent `end_session`; harness exits non-zero when it is sent. Opened 2026-09-11.

Resume and prompt cache:
- #66005 `--resume` drops the session's `--effort` level, invalidating the prompt cache. Opened 2026-06-07, c10.
- #69568 Resume/fork includes completed-turn thinking signatures, inflating context usage toward limit. Opened 2026-06-19, c11.
- #87106 Resuming completed subagents replays full transcript per message; burned 75% of weekly limit in one afternoon. Opened 2026-08-16, c3.
- #65796 Workflow (multi-agent) resume restarts from beginning after auto-compaction; silently re-runs completed agents. Opened 2026-06-06, c15.
- #74599 Workflow `resumeFromRunId` re-executes successful `agent()` calls when script uses `pipeline()` or `parallel()`. Opened 2026-07-05, c7.
- #95076 Workflow `resumeFromRunId` re-runs every completed agent after first parallel fan-out (40 redundant opus agents, 231M tokens). Opened 2026-09-17, c3.
- #87966 Prompt cache lookup fails intermittently mid-session; `cache_read` pinned to stable-prefix boundary; 89 full-context rewrites over 9 days (~59M excess cache_creation tokens). Opened 2026-08-19, c10.
- #66115 Feature request: auto-compact on idle timeout to prevent cache-expiry cost. Opened 2026-06-07, c15.

Rate limits and quota:
- #16157 "Instantly hitting usage limits with Max subscription". Opened 2026-01-03, c1499 (most-commented open Claude Code issue in the search results).
- #38335 Max plan session limits exhausted abnormally fast since 2026-03-23 (CLI usage). Opened 2026-03-24, c877.
- #54750 Current session limit reaches 100% despite low visible local usage. Opened 2026-04-29, c21.
- #41788, #45756, #42249, #42052: Max plan quota exhausted in minutes or hours (April 2026). c42-c60 each.
- #75408 `statusLine` `rate_limits.five_hour.used_percentage` drifts across concurrently open terminals. Opened 2026-07-07, c2.
- #86393 VS Code: dismissing the usage-limit banner does not hold (`allowed` events clear `dismissedRateLimitKey`). Opened 2026-08-13.
- #91857 Agent SDK: `rate_limit_event` already carries per-bucket `unifiedWindows` utilization at `status=allowed`; `SDKRateLimitInfo` does not declare it (follow-up to #50518). Opened 2026-09-03.
- #13585 Feature request: quota information access in the CLI. Opened 2025-12-10, c28.
- #22435 Inconsistent and undisclosed quota accounting changes in Max plan. Opened 2026-02-01, c14.
- #88178 Claude Desktop: 5 h 46 m cumulative waiting in one day (23 silent ~15-minute stalls). Opened 2026-08-20, c12.

Windows and paths:
- #14828 Windows: console window flashes when executing tools. Opened 2025-12-20, c64.
- #18748 Bash tool returns empty output for shell scripts on Windows (MINGW64). Opened 2026-01-17, c41.
- #3381 Windows VS Code extension: "cannot open _claude_fs_right:<file>" relative path error. Opened 2025-07-12, c56.
- #1669 Claude Code loses track of which directory it is in. Opened 2025-06-05, c50.
- #55206 Cowork on Windows: bash sandbox can create files on mounted host folder but unlink denied. Opened 2026-05-01, c15 (Cowork is a desktop feature).
- #45297 Cowork: folder does not support UNC paths under Windows. Opened 2026-04-08, c31.
- Changelog-side Windows fixes in window: v2.1.283 startup slow under SDK hosts when managed settings deny many reads on Windows drives under WSL (raw line 482); v2.1.285 project/local `env` no longer sets `ALLUSERSPROFILE`, `SystemDrive`, `CommonProgramFiles*` (WebFetch).

General:
- #91870 "Mods - make Claude 10x more extensible" (feature). Opened 2026-09-03, c254.
- #69044 "User feedback: recurring errors documented over months of daily use". Opened 2026-06-17, c88.

### 5. Claude Code notes on version numbering vs. Codex

- Claude Code keeps `2.1.X` as the only numbering. A harness can gate on `claude --version` but has no semver signal for breaking changes; the changelog labels breaking changes inconsistently. Gate on features with version floors from the docs (e.g. `claude-sonnet-5-5` needs 2.1.284+).

## Codex CLI

### Release cadence and numbering
- Tags `rust-v0.X.Y` on github.com/openai/codex (npm package `@openai/codex` per the CLI reference; npm tag not checked in this run).
- Stable releases since 2026-07-01: 43 (`rust-v0.142.5` to `rust-v0.162.1`). Pre-releases since 2026-07-01: 285 (`-alpha.N`, `-alpha.N.M`). Latest seen: `rust-v0.162.1` (2026-10-09). Newest alpha: `rust-v0.163.0-alpha.5` (2026-10-10).
- Stable minor cadence: roughly every 2-5 days in Sept-Oct 2026 (e.g. 0.159.0 Sept 29, 0.160.0 Oct 1, 0.161.0 Oct 7, 0.162.0 Oct 8).
- Pin and detect: `codex --version` (not verified in this run). Update command: `codex update` ("Check for and apply a Codex CLI update when the installed release supports self-update") per developer-commands page (WebFetch). Not found in that page: an auto-update check, a config key or env var to disable update checks, or a version pin for the CLI. npm/brew/winget version commands not found.
- Managed-update features in releases: `codex app-server daemon update` and configurable daemon update schedules (0.155.0); administrator controls for in-app updates for enterprise plans (0.146.0); Windows managed updates for the background server (0.154.0).

### 1. Breaking changes, renamed or removed flags, changed defaults (release bodies unless noted)

Removed or renamed:
- 0.147.0: `codex exec --full-auto` removed; use `--sandbox workspace-write`.
- 0.154.0: deprecated `codex mcp-server` entry point removed.
- 0.159.0: automatic follow-up prompt suggestions removed, with the `tui.prompt_suggestions` setting. Bundled `plugin-creator` skill removed.
- 0.161.0 (WebFetch of the changelog page, not in release body): "Remove repository-local Codex guidance, skills, and environment config" (#49713).
- 0.162.0 (WebFetch of the changelog page): strict config validation rejects unknown TUI keys (#50525); stale TUI config keys could fail startup.
- 0.143.0: `thread/rollback` marked deprecated (#29928, release PR list). "Remove partial-history subagent forks" (0.162.0 list, WebFetch).

Defaults:
- 0.143.0: remote plugins enabled by default; MCP tools use tool search by default (#29486).
- 0.144.0: code mode moved to hosted mode by default (#31500).
- 0.145.0: GPT-5.6 Sol is default Bedrock model (#32288).
- 0.146.1: safer automatic-review defaults for cyber-capable models.
- 0.147.0: MCP 2026-07-28 protocol opt-in (not default).
- 0.152.0: planning tool disabled by default; enable with `tools.update_plan.enabled = true` (#41744).
- 0.153.0: `tui.disable_paste_burst` replaces top-level setting (top-level still works as fallback).
- 0.154.0: remote resume and fork preserve saved permissions; fresh sessions and forks respect server model defaults unless overridden.
- 0.155.1: new local TUI sessions leave reasoning summaries disabled by default.
- 0.156.0: voice conversations enabled by default (F8 toggle); worktree support enabled by default. 0.157.0: fullscreen transcripts enabled by default.
- 0.158.0: terminal input approval enabled by default for elevated commands.
- 0.159.1: GPT-6.1 Sol added as default model in bundled catalog and Bedrock Mantle and Runtime catalogs.
- 0.161.0: GPT-6.1 Sol is the default model in bundled and Bedrock catalogs (#49318, #49339). Daybreak opt-in via `--enable cli_daybreak` or `features.cli_daybreak=true`; `daybreak=true` alone insufficient.
- 0.162.0: new TUI threads respect server model and reasoning-summary defaults; Retry-After honored for Responses and WebSocket failures (#50418, #51440).

Protocol and output changes (harness-facing, release PR lists):
- 0.148.0: `model/list` exposes multi-agent versions (#37433).
- 0.162.0: command execution output consolidated into `aggregated_output` (#50402); initial messages removed from session configuration events (#50360); partial answer message phase added (#51241); base instructions sent as Responses input messages (#51156); persisted command output capped at 64 KiB (WebFetch).
- 0.161.0: thread resume returns authoritative replay history (#49599); experimental thread prediction protocol types (#49480).
- 0.146.0: app-server serialization changes; Code Mode WebSocket connection; "pinned threads" (#34840).
- 0.149.0: SDK users can pass exact CLI config overrides and select `max` or `ultra` reasoning effort (#38817, #39662).
- 0.153.0: app-server thread metadata includes nullable `model` and `reasoningEffort`; `request_user_input_async` (#42151).
- 0.157.0 / 0.158.0: command completion events include early output; process-launch failures reported to clients (#47529, #47665).

Sandbox and approvals (behavioural, may affect unattended runs):
- 0.148.0: sandbox restrictions fail closed for denied or unreadable paths across Linux and Windows.
- 0.160.0 / 0.161.0: approved filesystem escalation can grant broader write access, keeping denied reads; background tasks keep originating turn's permissions (#49353).
- 0.162.0: rejects sandbox-writable bubblewrap executables from PATH (#51211).

### 2. New features useful to a harness (release bodies unless noted)
- app-server methods: `thread/fork` with optional `turn_id` (0.143.0, #30277); `thread/items/list` (#29705); `thread/list` descendant threads (#29591); environment info RPC (#30291); `model/list` (see above); `modelProvider/capabilities/read`, `experimentalFeature/list`, `collaborationMode/list` (WebFetch of app-server page); `thread/tokenUsage/updated` notification (WebFetch); thread pinning (0.146.0, #34840); thread queue APIs experimental (0.148.0, #38456); `thread/shellCommand` timeouts over one hour (0.152.0, #41384); thread history pagination (0.145.0, #33364; 0.159.0 #48151); app-server reverting paginated threads (0.148.0, #38440); plugin reconciliation API (0.153.0, #41949).
- app-server stdio transport (default `--listen stdio://`), newline-delimited JSON, schema generators `codex app-server generate-ts` and `codex app-server generate-json-schema` (WebFetch of app-server page). Page states app-server "command and WebSocket transport are experimental and aren't supported for production workloads" and that some fields need `capabilities.experimentalApi`.
- `codex exec --json` (alias `--experimental-json`): newline-delimited JSON events, one per state change (WebFetch of developer-commands page). Event types not listed on that page.
- `codex exec fork` (0.148.0, #37367); `codex exec resume` handles compressed rollouts (0.153.0).
- `codex exec --cyber-access-program` and TS SDK `cyberAccessProgram` (0.161.0, WebFetch).
- Rate-limit related: usage-limit reset credits (type, expiry, choice of which to redeem) (0.144.0, #30488); rate-limit banners with actions (0.152.0, #41742); earlier warning when Plus or Team has less than half of ~5 h allowance left (0.153.0, #42142); `[app-server] Include reset-credit details in rate limits` (0.143.0, #30395); honors workspace spend controls in rate-limit handling (0.145.0, #33187); fractional rate limits in thread history (0.147.0, #36083); exact per-response usage in raw app-server events (0.145.0, #32985); `/usage` analytics dashboard (0.156.0, #45764).
- Hooks: async hooks and MCP tool invocation from hooks (0.148.0, #37533, #38705); `Interrupt` hooks (0.150.0, #40511).
- `codex agents` dashboard (0.149.0); `codex queue` for messaging existing sessions (0.149.0, #39092); `--worktree` sessions (0.154.0, experimental; default-on 0.156.0); `--approve-for-me` automatic review (0.147.0, #36373).
- `codex remote-control pair` (0.143.0, #29913); `codex doctor` covers endpoint protection, proxy and update connectivity (0.149.0).
- `codex sandbox` and sandbox fail-closed behaviour (see above).

### 3. Model lifecycle (Codex/OpenAI)

Source: developers.openai.com/api/docs/deprecations (redirected from platform.openai.com/docs/deprecations; WebFetch; the page is not dated itself, WebFetch used section dates as deprecation dates).

Deprecations:
- 2026-10-01 (section date): `gpt-5.3-codex` (shutdown 2027-04-01, replacement `gpt-6-sol`); `gpt-5.4-nano` (shutdown 2027-04-01, replacement `gpt-6-luna`); `gpt-5.1` (shutdown 2027-04-01, replacement `gpt-6-sol`). Also TTS models (`tts-1`, `tts-1-hd`, `gpt-4o-mini-tts-*`) to `gpt-realtime-2.1-mini`, shutdown 2027-01-06.
- 2026-09-11 (section date): `gpt-5.4-cyber`, shutdown 2026-10-01; replacement described as "the most capable cyber model available to you".
- 2026-08-26: transcription models (`whisper-1`, `gpt-4o-transcribe`, `gpt-4o-mini-transcribe`, `gpt-4o-transcribe-diarize`) to `gpt-live-transcribe` or `gpt-transcribe`, shutdown 2027-02-26.
- 2026-07-20: realtime and audio models (`gpt-realtime`, `gpt-audio`, `gpt-4o-audio`, `gpt-4o-realtime`, mini variants) to `gpt-realtime-2.1` / `gpt-audio-1.5` family, shutdown 2027-01-20.

Current CLI model set (release notes, Codex CLI):
- Default: GPT-6.1 Sol (0.159.1 for bundled catalog; 0.161.0 for bundled and Bedrock catalogs). Also in Bedrock catalogs from 0.159.1.
- Available: GPT-6 Sol, GPT-6 Luna (0.157.0; Bedrock support included); GPT-6 Sol and Luna picker entries (0.156.1); GPT-6-Astra (0.154.0, picker and Bedrock); GPT-5.6 Sol, Terra, Luna (0.144-0.145, Bedrock; 0.145 default Bedrock model GPT-5.6 Sol); GPT-6.1 Sol Ultrafast in ChatGPT Work (learn.chatgpt.com changelog, 2026-10-08; not in release bodies).
- Note from the deprecation page: `gpt-5.3-codex` and `gpt-5.1` (listed) and `gpt-5.4-nano` are the older models being replaced by `gpt-6-sol` and `gpt-6-luna`.

Listing programmatically:
- `model/list` on app-server. Params: `limit`, `includeHidden`. Response: `data[]` with `id`, `model`, `displayName`, `hidden`, `defaultReasoningEffort`, `supportedReasoningEfforts[]` (`reasoningEffort`, `description`), `inputModalities`, `supportsPersonality`, `isDefault`; plus `nextCursor`. Default returns picker-visible models only (WebFetch of app-server page, learn.chatgpt.com).
- The CLI reference does not document a command that lists models without app-server.

### 4. Release and quota-related issues, Codex (open at search time, 2026-10-11)

Exec hangs (headless):
- #34397 `codex exec` hangs indefinitely; `codex_models_manager` logs "timeout waiting for child process to exit"; no error. Opened 2026-07-20, c2.
- #50775 `codex exec` hangs when model response stream stalls; no idle timeout fires. Opened 2026-10-04, c1.
- #51258 Responses HTTP can hang indefinitely before response headers; timeout and retries never engage. Opened 2026-10-06, c5.
- #31376 Responses HTTP requests can stall before response headers without bounded retry (Windows, Linux, macOS). Opened 2026-07-07, c16.
- #50141 Windows: `codex exec` stays alive after `turn.completed` until a process started with `Start-Process -RedirectStandardOutput/-RedirectStandardError` exits. Opened 2026-10-02.
- #27019 Hangs at "Reading additional input from stdin..." in non-TTY (`TERM=dumb`). Opened 2026-06-08, c1. Duplicate-like: #48716 (2026-09-27, c0).
- #19945 `codex exec` silently crashes with no output when stdio detached from TTY (0.124.0+). Opened 2026-04-28, c2.
- #14115 `codex exec` on macOS initializes external MCP and lists tools but never sends `tools/call`. Opened 2026-03-09, c5.
- #7852 `--full-auto` and `--sandbox workspace-write` cause indefinite hang with orphaned child processes. Opened 2025-12-11, c5.
- #15830 `codex exec` hangs under systemd `--user` on WSL2, works in normal shell. Opened 2026-03-26, c1.
- #16168 Codex CLI hangs on Ubuntu during normal prompts. Opened 2026-03-29, c3.

Streams and reconnects:
- #18960 Reconnect loop in Codex App: websocket closed by server before `response.completed`. Opened 2026-04-22, c62.
- #35420 Stream repeatedly disconnects when Windows workspace is OneDrive-backed and OneDrive degraded. Opened 2026-07-26, c36.
- #52814 Repeated mid-task stream disconnections in long `gpt-6.1-sol` conversation (macOS). Opened 2026-10-10, c3.
- #52567 CLI goal hits context overflow after WebSocket retries and fails on resume. Opened 2026-10-09, c0.
- #9936 Azure: "stream disconnected before completion: response.failed event received". Opened 2026-01-26, c23.

Resume and sessions:
- #53234 Resume history filtered by `model_provider`; switching providers hides every past session. Opened 2026-10-12.
- #25215 Long threads unrecoverable when rollout JSONL grows beyond resume/list limits. Opened 2026-05-30, c8.
- #51562 0.148.0: tool-backed `codex exec` ends with thread-not-found rollout persistence error. Opened 2026-10-07.
- #53165 macOS 26.1007: existing durable thread loses terminal and file tools after reconnect; resume logs `environmentIds` as `[]`. Opened 2026-10-11.
- #53209 Ghost chats fail to resume with missing rollout (macOS Desktop). Opened 2026-10-11.

Exec JSON, SDK, protocol:
- #53017 `@openai/codex-sdk`: TS item types drifted from exec JSONL wire format. Opened 2026-10-11, c2.
- #50596 Expose model-request lifecycle, timing and per-request usage in `codex exec --json`. Opened 2026-10-03.
- #50593 Expose context compaction lifecycle in `codex exec --json`. Opened 2026-10-03.
- #50588 Preserve message phase, native item IDs, tool durations, structured model rerouting in `codex exec --json`. Opened 2026-10-03.
- #50586 Expose reported token usage for failed and interrupted runs in `codex exec --json`. Opened 2026-10-03.
- #50936 `codex exec` does not load remote-installed plugins; their skills missing from headless sessions. Opened 2026-10-04.
- #51716 Fragmented WebSocket requests reach app-server as incomplete JSON; attachments hang (Windows). Opened 2026-10-07.
- #51813 Windows desktop hangs when a stale `CODEX_CLI_PATH` selects an incompatible app-server. Opened 2026-10-07.

Rate limits and quota:
- #34035 "Make the temporary removal of the 5-hour usage limit permanent". Opened 2026-07-18 (comment count not captured).
- #28823 5-hour usage meter appears to consume much faster than comparable historical usage. Opened 2026-06-18, c19.
- #53046 Sol Ultra 5-hour allowance exhausted within 2-3 minutes. Opened 2026-10-11, c1.
- #52382 macOS App: task marked complete while background CLI inference continues and exhausts usage limit. Opened 2026-10-09, c2.
- #35259 Desktop repeatedly re-enters the model during wait/status polling, consuming credits. Opened 2026-07-24, c28.
- #34971 Repeatedly reprocesses massive cached context in long sessions (regression). Opened 2026-07-23, c13.
- #52796 "Selected model is at capacity" on ~80% of turns for 2 days, all models (macOS). Opened 2026-10-10, c7. Related: #52812 `server_overloaded` on trivial first turn (2026-10-10, c1); #28507 "Selected model is at capacity" (2026-06-16, c57).

Cache:
- #20301 Low cache hit rate with GPT-5.5. Opened 2026-04-30, c17.
- #39392 Codex App with `gpt-5.6-sol` aborts with unsupported `prompt_cache_retention`. Opened 2026-08-19, c39.
- #31870 `gpt-5.6-sol` through Azure fails every turn with `X-OpenAI-Internal-Codex-Responses-Lite`. Opened 2026-07-09, c51.
- #31882 `gpt-5.6-sol/terra/luna` hardcode `use_responses_lite` / `multi_agent_version`, causing 400s on Azure and likely other non-ChatGPT providers. Opened 2026-07-09, c12.

App-server crashes (Windows-heavy):
- #37164 Windows Desktop crashes ~10 s after UI loads with empty `CODEX_HOME` (0xc0000409, utility process). Opened 2026-08-05, c15.
- #30884 Desktop crashes "Codex app-server websocket closed (code=3221225501)" while CLI works (Windows 11 25H2). Opened 2026-07-02, c8.
- #26440 `app-server` repeatedly exits on Windows; plugin metadata and PowerShell `shell_snapshot` WARNs. Opened 2026-06-04, c8.
- #12962 `app-server` crashes on Windows 11 build 26200 (hardware capability error). Opened 2026-02-26, c7. This matches the maintainer's OS build.
- #48466 Windows 26.924: every cold startup stalls on Loading; restarting only app-server restores UI. Opened 2026-09-26, c20.
- #39226 Composer and app RPC actions intermittently fail with opaque `{"source":"owned"}` error (Windows). Opened 2026-08-18, c9.
- #53192 Desktop app 26.1007.30912: opening existing session crashes renderer and causes reload loop. Opened 2026-10-11, c1.

Windows paths and launching:
- #40752 Windows Desktop fails to start after update to v26.820.60940: "Unable to locate Codex CLI" and `spawn EINVAL` on `.cmd` wrapper. Opened 2026-08-26, c87. Relevant to the AGENTS.md rule about npm `.cmd` shims.
- #40715 Windows: "invalid transport in mcp_servers.codex_app"; beta 26.727.40816 works. Opened 2026-08-25, c71.
- #40819 Desktop 26.820.7780.0: resuming WSL-hosted threads fails with "invalid transport in mcp_servers.codex_app". Opened 2026-08-26, c68.
- #41463 Windows + WSL: cannot create projects; `AbsolutePathBuf` deserialized without base path. Opened 2026-08-29, c63.
- #27117 Windows standalone update from `pwsh` inherits `PSModulePath` into `powershell.exe`, causing `Get-FileHash` to fail. Opened 2026-06-09, c44.
- #29072 `apply_patch` fails because `codex-windows-sandbox-setup.exe` cannot launch from package path. Opened 2026-06-19, c46.
- #49731 Windows app "Run agent in WSL": every command fails with "No such file or directory" (arg0 helper dir deleted). Opened 2026-09-30, c41.
- #53190 Windows + WSL: `node_repl` rejects Linux cwd; primary runtime catalog mismatch. Opened 2026-10-11, c1.

Model catalog and picker (matters for `model/list` consumers):
- #53197 macOS Desktop 26.1007: `model/list` expires after 30 s in queue; Local Work startup repeatedly times out on `plugin/installed`. Opened 2026-10-11, c1.
- #51663 Resolved model catalog removes the entire GPT-6 family despite bundled catalog and successful direct request. Opened 2026-10-07, c1.
- #51308 Windows Desktop: GPT-6 Sol, GPT-6.1 Sol and GPT-6 Luna missing from model picker. Opened 2026-10-06, c9.
- #53219 Desktop model picker hides GPT-6.1 Sol after switching models. Opened 2026-10-11, c1.
- #52628 JetBrains/IntelliJ model picker stale: only GPT-5.4/5.3/5.2 shown while CLI exposes GPT-6.1 Sol. Opened 2026-10-09.
- #51011 Existing Work conversation reopens in Chat after restart and loses GPT-6.1 Sol access. Opened 2026-10-05.

Most upvoted or commented open Codex issues (comment counts from `gh search`; reactions not exposed):
- #40752 (c87), #48237 "unexpected status 401 Unauthorized" (opened 2026-09-25, c97), #51601 Windows app sandbox setup sharing violation (2026-10-07, c122), #19464 "Support 1M token context for GPT-5.5" (2026-04-24, c132), #20214 Windows desktop freezes/stutters (2026-04-29, c114), #25719 macOS `syspolicyd`/`trustd` runaway (2026-06-01, c90).
- #41975 macOS CLI/Desktop returns 401 while `auth.json` access token accepted via curl (2026-09-01, c22) — relevant to auth-token handling (do not read tokens; flagged only).

## Unknown or unclear
- Claude Code: whether `claude -p` and the SDK receive the same `rate_limit_event` fields documented in the SDK types (#91857 says `SDKRateLimitInfo` lacks `unifiedWindows`).
- Claude Code: exact `rate_limit_event` JSON schema; only changelog descriptions seen.
- Claude Code: whether the subscription login can call `GET /v1/models`; the API page documents only API-key use.
- Claude Code: default cache TTL. Changelog shows `promptCacheTtl` and `subagentPromptCacheTtl` options but no default change in the window; the default was not restated.
- Claude Code: `CLAUDE_CODE_SUBAGENT_MODEL` full description (table cut off in WebFetch).
- Claude Code: changelog 2.1.287 entry says `alwaysLoad: false` defers all tools; raw text not checked line by line.
- Codex: whether `codex` checks for updates on startup, and whether any setting disables it. Not found on the CLI reference page (WebFetch).
- Codex: `codex --version` output and npm tag names not checked.
- Codex: exec `--json` event type list and schema. Not on the reference page; only issue #53017 (TS SDK types drift) and the open exec-JSON gap issues.
- Codex: deprecation page date semantics. Section dates (2026-10-01, 2026-09-11, 2026-08-26, 2026-07-20) may be announcement dates, not shutdown dates; WebFetch treated them as deprecation dates.
- Codex: `GPT-6.1 Sol Ultrafast` availability in the CLI (learn.chatgpt.com changelog only).
- Codex: 0.161.0 "remove repository-local Codex guidance, skills and environment config" (#49713) and 0.162.0 "strict config validation rejects unknown TUI keys" (#50525) come from WebFetch of the changelog page, not raw release bodies.
- Codex: whether app-server `model/list` returns hidden models with `includeHidden` only.
- Codex: the cache TTL default for GPT-6 family (not checked).
- Claude model deprecation page: Haiku 4.5 row shows Active with "Not sooner than October 15, 2026". Read as written.

## Questions worth asking
1. Should Faber pin the Claude Code CLI (`DISABLE_UPDATES=1` or `requiredMaximumVersion`) and check `claude --version` at startup, or accept auto-updates and gate features by version floor?
2. Should Faber pass an explicit `--permission-mode` for `-p` and SDK runs, given the default changed to auto mode on third-party providers and telemetry-off installs (v2.1.283, v2.1.285)?
3. Should Faber set `CLAUDE_CODE_ENABLE_TODO_TOOLS=1` if its event parser needs TodoWrite or TaskCreate events on Opus 5.5 or Sonnet 5.5, which lost them in v2.1.233?
4. Which Codex transport should Faber use: `codex app-server` (documented as experimental, not for production) or `codex exec --json` (experimental flag, no documented event schema, open gap issues #50586-#50596)?
5. Should Faber's Claude path use `claude -p --output-format stream-json` or the Agent SDK, given #91982 (no user frame on stdout for cross-session messages) and #100353 (keep-alive disabled after one connection error since 2.1.213)?
6. Should the quota display in Faber rely on Claude's `rate_limit_event` and status-line `rate_limits`, given open issues #75408, #86393, #91857, or show "unknown" until verified?
7. Should Faber's Codex default follow the CLI default (GPT-6.1 Sol), and should it plan a move off `gpt-5.3-codex` and `gpt-5.1` before shutdown (2027-04-01, per the deprecation page)?
8. Should Faber add a Windows check for Codex app-server crashes (#12962 matches build 26200) and the `.cmd`/`spawn EINVAL` class (#40752) before launching the CLIs?

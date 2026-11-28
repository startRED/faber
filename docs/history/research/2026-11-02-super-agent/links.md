# Research notes: agent workspaces, proactive agents, and Cline

Research checked 2026-11-02. Each factual claim is linked to the page it came from. `UNREAD` means the referenced page itself could not be opened; nearby facts are attributed only to the accessible source named.

## 1. DeepSeek Harness X post and linked article

Sources: [Original X post](https://x.com/DeepSeekHarness/status/2116857257245987236) (**UNREAD**: X returned 403); [article mirror on Xitter/Teapawt](https://xitter.cf/DeepSeekHarness/article/2116857257245987236); [linked prototype repository](https://github.com/deepseek-ai/dsh-superagent-prototype).

### Summary (5 lines)

- The article argues that human attention, rather than model capability alone, limits how many concurrent agent tasks people can supervise. [Article mirror](https://xitter.cf/DeepSeekHarness/article/2116857257245987236)
- It proposes one long-lived Router for human interaction and global context, with isolated Worker sessions for parallel task execution. [Article mirror](https://xitter.cf/DeepSeekHarness/article/2116857257245987236)
- Environmental events become Triggers; deterministic filters screen noise before model-based policy decides whether a candidate deserves action. [Article mirror](https://xitter.cf/DeepSeekHarness/article/2116857257245987236)
- It adds asynchronous start/stop work, event pass-through to subscribed workers, context caching/retrieval, and a separate Watchdog for recovery and progress checks. [Article mirror](https://xitter.cf/DeepSeekHarness/article/2116857257245987236)
- The article says the prototype implements only parts of the design and is not an official DeepSeek product; it is machine-translated from Chinese. [Article mirror](https://xitter.cf/DeepSeekHarness/article/2116857257245987236)

### Key mechanisms

- **Two-level execution:** Router handles people, cross-task coordination, dispatch, and global memory; it does not execute tasks. Workers have isolated contexts and tools, each dedicated to a task, and report key results to the Router. [Article mirror](https://xitter.cf/DeepSeekHarness/article/2116857257245987236)
- **Fast/slow admission:** code filters candidate events using source/group/user/bot allowlists and modes such as respond-only-to-explicit-requests, candidate, or ignore; an LLM evaluates the survivors against semantic policies. [Article mirror](https://xitter.cf/DeepSeekHarness/article/2116857257245987236)
- **Thread subscriptions and pass-through:** Router assigns a Worker to a Thread on first event; later events go directly to its subscribed Worker(s). Threads and Workers can have many-to-many subscriptions; Workers report conclusions to Router. [Article mirror](https://xitter.cf/DeepSeekHarness/article/2116857257245987236)
- **Inline & Spill / Chain of Recall:** initially inject IDs and summaries, add nearby context likely to be useful, and leave oversized/rare material outside the context window behind references. Linked IDs let the model follow related objects on demand. [Article mirror](https://xitter.cf/DeepSeekHarness/article/2116857257245987236)
- **Speculative work:** start low-risk background investigation before knowing whether the result will be needed; deliver useful results and discard the rest. The article’s example is silently reproducing reported bugs and surfacing a fix only when verified. [Article mirror](https://xitter.cf/DeepSeekHarness/article/2116857257245987236)
- **Full-duplex interaction:** long commands/tools use asynchronous `start()` and `stop()`; completion arrives later as an event, allowing the Router to accept further user messages while work continues. The article says this Harness mechanism exists but is not enabled by default. [Article mirror](https://xitter.cf/DeepSeekHarness/article/2116857257245987236)
- **Watchdog:** an independent agent session checks infrastructure liveness and whether workers are actually progressing or drifting, and can retry transient failures or adjust prompts/triggers. [Article mirror](https://xitter.cf/DeepSeekHarness/article/2116857257245987236)
- **Compute/state/environment separation:** proposed cloud-hosted, stateless loop plus persistent database and lightweight endpoint daemons, so a cloud Router can use personal or intermittently online machines (BYOM). [Article mirror](https://xitter.cf/DeepSeekHarness/article/2116857257245987236)

### Concrete details

- Named concepts: `Router`, `Worker`, `Trigger`, `Thread`, `Watchdog`, `Fast Path`, `Slow Path`, `Inline & Spill`, `Chain of Recall`, `BYOM`; tool API sketch uses `start()` and `stop()`. [Article mirror](https://xitter.cf/DeepSeekHarness/article/2116857257245987236)
- The article links an implementation prototype at [deepseek-ai/dsh-superagent-prototype](https://github.com/deepseek-ai/dsh-superagent-prototype); the article’s explicit notice says it validates the approach and is not an official product. [Article mirror](https://xitter.cf/DeepSeekHarness/article/2116857257245987236)
- In an internal simulated workload, five runs per design showed p50/p95 completion of 12.4/15.7 minutes for Super-agent, versus 20.4/29.5 minutes for a single session with a neutral subagent prompt and 13.8/29.0 with a delegation-first prompt. Super-agent used 23.2M cache-read tokens/run versus 6.1M and 10.8M respectively. [Article mirror](https://xitter.cf/DeepSeekHarness/article/2116857257245987236)
- The article reports router batching from mean batch size 1.0 to 2.8 as trigger arrivals rose from about 1 to 40/minute; amortized inference time per decision fell from 6.4s to 1.6s. [Article mirror](https://xitter.cf/DeepSeekHarness/article/2116857257245987236)
- It recommends short IM replies (example hard cap: 100 words) and fuller structured responses in asynchronous channels such as GitHub, email, and documents. [Article mirror](https://xitter.cf/DeepSeekHarness/article/2116857257245987236)

### Limits and weak points

- **Original post:** **UNREAD**. X returned 403, and the attempted Nitter/XCancel/Twiiit mirrors were inaccessible. The linked article was readable through the Xitter/Teapawt mirror; this is not a direct X fetch. [Original X post](https://x.com/DeepSeekHarness/status/2116857257245987236) · [accessible article mirror](https://xitter.cf/DeepSeekHarness/article/2116857257245987236)
- Article measurements are preliminary and indicative only: synthetic messages/environments, simulated coding work, and five runs per design. They measure scheduling/coordination, not code quality or real-world task completion. [Article mirror](https://xitter.cf/DeepSeekHarness/article/2116857257245987236)
- Authors call scaling the Router, long-term memory, product UI for sessionless work, cross-agent communication, and security/privacy open problems. Cloud loop access to endpoint credentials/code creates a prompt-injection and compromise risk. [Article mirror](https://xitter.cf/DeepSeekHarness/article/2116857257245987236)
- The article itself says the implementation is simplified and scoped to a small team; it does not establish that the broader architecture is production-ready. [Article mirror](https://xitter.cf/DeepSeekHarness/article/2116857257245987236)

## 2. PandaCode repository

Sources: [README](https://github.com/aicodeking/pandacode/blob/main/README.md); [bots documentation](https://github.com/aicodeking/pandacode/blob/main/docs/bots.md); [schedules documentation](https://github.com/aicodeking/pandacode/blob/main/docs/schedules.md); [provider integrations](https://github.com/aicodeking/pandacode/blob/main/docs/providers.md); [repository activity API](https://api.github.com/repos/aicodeking/pandacode); [latest release](https://github.com/aicodeking/pandacode/releases/latest).

### Summary (5 lines)

- PandaCode is a local desktop workspace that runs installed coding-agent CLIs/harnesses side by side with projects, Git, terminals, browser, tasks, teams, and bots. [README](https://github.com/aicodeking/pandacode/blob/main/README.md)
- A bot is a named, persistent agent with a role/instructions/goals, its own workspace and ordinary task thread, plus optional memory, channels, approvals, and schedules. [Bots docs](https://github.com/aicodeking/pandacode/blob/main/docs/bots.md)
- Router-like bot collaboration uses the `bot_message_bot` tool to enqueue work to another bot’s desktop task and return the result to the sender. [Bots docs](https://github.com/aicodeking/pandacode/blob/main/docs/bots.md)
- External channels include local web chat, Telegram, Slack, Discord, and WhatsApp Cloud API; bot runs use the same harness/provider infrastructure as regular tasks. [Bots docs](https://github.com/aicodeking/pandacode/blob/main/docs/bots.md)
- It is free/open source under a modified MIT-style PandaCode License; repository metadata checked 2026-11-02 showed 1,017 stars, 181 forks, pushed that day, and latest release `v0.3.91`. [README/license](https://github.com/aicodeking/pandacode/blob/main/README.md) · [GitHub API](https://api.github.com/repos/aicodeking/pandacode) · [releases](https://github.com/aicodeking/pandacode/releases/latest)

### Key mechanisms

- **Bot identity and continuity:** profile fields include name, role, instructions, goals, optional Markdown “soul” (personality), avatar, harness, and model. `bot_threads` maps an external channel/thread to an ordinary PandaCode task; the task’s run transcript, approvals, queue, steering, usage limits, and history are reused. [Bots docs](https://github.com/aicodeking/pandacode/blob/main/docs/bots.md)
- **Router-to-worker delegation:** `bot_message_bot(to, text)` rejects self-messaging, queues behind target work, records markers in both tasks, and posts the target’s response back as `Update from <bot>`; chains are capped at three hops. [Bots docs](https://github.com/aicodeking/pandacode/blob/main/docs/bots.md)
- **Human-readable shared channels:** local group channels route unmentioned posts to a lead, or explicit `@mentions` to at most two bots in parallel. Each assignment appears in the bots’ ordinary task history; people can stop, continue, or reassign work. [Bots docs](https://github.com/aicodeking/pandacode/blob/main/docs/bots.md)
- **Schedules and heartbeat:** bot schedules can recur every N minutes, daily, or weekdays and launch the same kind of task as a channel message; scheduled tasks survive restarts. Bot goals can also be checked read-only through heartbeat. [Bots docs](https://github.com/aicodeking/pandacode/blob/main/docs/bots.md)
- **General task schedules:** tasks can get one-time, interval, daily, weekly, or “after another task” messages; the message is an ordinary task prompt. Run history, max-runs, pause, snooze, manual run, and workflow chaining are supported. [Schedules docs](https://github.com/aicodeking/pandacode/blob/main/docs/schedules.md)
- **Provider/harness routing:** launches the installed CLI directly; the user’s normal CLI login remains the authentication source for Claude Code, Codex, and many other harnesses. Harness and model are selected per task/bot. Named connections can separate profiles such as work/personal. [README](https://github.com/aicodeking/pandacode/blob/main/README.md) · [Providers docs](https://github.com/aicodeking/pandacode/blob/main/docs/providers.md)
- **Channel trust/safety:** unknown senders must pair; bots default to approval mode; a Guard applies deny/ask/allow rules and always asks about listed risky operations; `/stop` or app pause is a kill switch. Channel secrets are held outside prompts and the webview. [Bots docs](https://github.com/aicodeking/pandacode/blob/main/docs/bots.md)
- **Local-first storage:** projects, conversations, and settings are local; bot memory/activity/config uses SQLite; channel/API secrets use OS credential storage. [README](https://github.com/aicodeking/pandacode/blob/main/README.md) · [Bots docs](https://github.com/aicodeking/pandacode/blob/main/docs/bots.md)

### Concrete details

- **Core stack:** Tauri 2 + Rust + SQLite + plain TypeScript; native operating-system webview; no bundled Chromium, resident Node server, React runtime, or embedded editor engine. [README](https://github.com/aicodeking/pandacode/blob/main/README.md)
- **Representative harnesses:** Claude Code, Codex, Kimi Code, OpenCode, Pi/`omp`, GitHub Copilot CLI, DeepSeek `dsh`, Cline, Kilo Code, Devin CLI, Hermes, ZCode, and Muse Code. Many use their own logins/configuration; Cline is launched as `cline --acp`, DeepSeek as `dsh --profile acp`, Codex through `codex app-server`, and Claude via its CLI protocol. [README](https://github.com/aicodeking/pandacode/blob/main/README.md) · [Providers docs](https://github.com/aicodeking/pandacode/blob/main/docs/providers.md)
- **Bot tools:** `bot_remember`, `bot_forget`, `bot_recall`, `bot_schedule`, `bot_list_schedules`, `bot_cancel_schedule`, `bot_notify`, `bot_message_bot`. [Bots docs](https://github.com/aicodeking/pandacode/blob/main/docs/bots.md)
- **Tables/config:** `bots`, `bot_threads`, `bot_schedules`, `bot_memories`, `bot_activity`, `bot_tasks`; project tasks use `task_schedules` and `task_schedule_runs`. User schedule settings are a JSON row (`SchedulerSettings`). [Bots docs](https://github.com/aicodeking/pandacode/blob/main/docs/bots.md) · [Schedules docs](https://github.com/aicodeking/pandacode/blob/main/docs/schedules.md)
- **Files/formats:** app database `kingcode.sqlite3`; named provider connections in `connections.json`; provider settings in `provider-settings.json`; imported bot personality is UTF-8 Markdown (`soul.md`, max 64 KB). [README](https://github.com/aicodeking/pandacode/blob/main/README.md) · [Bots docs](https://github.com/aicodeking/pandacode/blob/main/docs/bots.md) · [Providers docs](https://github.com/aicodeking/pandacode/blob/main/docs/providers.md)
- **Provider config keys:** `CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `PI_CODING_AGENT_DIR`, `OPENCODE_CONFIG_DIR`; global/provider environment variables are also supported. API keys for optional plans/providers are kept separately in the app’s secrets store. [Providers docs](https://github.com/aicodeking/pandacode/blob/main/docs/providers.md)
- **Channels/protocols:** Telegram Bot API long polling; Slack Socket Mode WebSocket; Discord Gateway WebSocket; WhatsApp Cloud API webhook at `/whatsapp/<channel id>` with `X-Hub-Signature-256`; web chat listens locally on `127.0.0.1:47321` or next free port. [Bots docs](https://github.com/aicodeking/pandacode/blob/main/docs/bots.md)
- **License:** “PandaCode License,” described as MIT plus a condition that commercial/public forks state “This is a fork of PandaCode” and link to the repository. [README/license](https://github.com/aicodeking/pandacode/blob/main/README.md)
- **Activity snapshot (2026-11-02):** GitHub API reported 1,017 stars, 181 forks, 72 open issues, repository pushed 2026-11-02; latest release endpoint reported `v0.3.91`. Counts change over time. [GitHub API](https://api.github.com/repos/aicodeking/pandacode) · [latest release](https://github.com/aicodeking/pandacode/releases/latest)

### Limits and weak points

- Bots run while PandaCode is running; quitting stops their active work. A schedule missed while the app was closed runs once at next start; missed repeats are skipped. [Bots docs](https://github.com/aicodeking/pandacode/blob/main/docs/bots.md)
- Bot-to-bot collaboration is intentionally shallow (three hops); shared-channel fanout is capped at two explicitly mentioned bots. [Bots docs](https://github.com/aicodeking/pandacode/blob/main/docs/bots.md)
- Scheduled task delivery has finite behavior: 15-second clock loop, overdue schedules fire once after restart, and repeated delivery failures disable a schedule after three in a row. [Schedules docs](https://github.com/aicodeking/pandacode/blob/main/docs/schedules.md)
- Third-party channel credentials and remote messages create an attack surface; the documentation itself treats remote input as untrusted and requires pairing/approval safeguards. Full Access bypasses the bot Guard’s approval gate. [Bots docs](https://github.com/aicodeking/pandacode/blob/main/docs/bots.md)
- The app does not provide vendor model plans; users install/sign in to providers themselves, and usage visibility varies by harness. Local-first also means scheduled work does not continue when the app is quit. [README](https://github.com/aicodeking/pandacode/blob/main/README.md) · [Bots docs](https://github.com/aicodeking/pandacode/blob/main/docs/bots.md)
- License is not plain MIT: the extra fork attribution condition matters for public or commercial forks. [README/license](https://github.com/aicodeking/pandacode/blob/main/README.md)

## 3. Cline Desktop

Sources: [Cline Desktop docs](https://docs.cline.bot/usage/cline-desktop); [Agent Teams](https://docs.cline.bot/cli/agent-teams); [Subagents](https://docs.cline.bot/features/subagents); [Scheduling](https://docs.cline.bot/cli/scheduling); [Remote over SSH](https://docs.cline.bot/usage/cline-desktop-ssh).

### Summary (5 lines)

- Cline Desktop is an open-source workspace app that runs Cline agent sessions outside an IDE, focused on open-weight models. [Desktop docs](https://docs.cline.bot/usage/cline-desktop)
- It supports parallel sessions, provider/model choice, marketplace capabilities, imports from coding agents, and local or remote workspaces. [Desktop docs](https://docs.cline.bot/usage/cline-desktop)
- The Desktop page documents one-time and recurring schedules with a prompt, workspace, provider, model, and optional system prompt, timeout, and tags. [Desktop docs](https://docs.cline.bot/usage/cline-desktop)
- Subagents are available in Desktop; persistent Agent Teams are documented only for Cline SDK, CLI, and Kanban. [Subagents](https://docs.cline.bot/features/subagents) · [Agent Teams](https://docs.cline.bot/cli/agent-teams)
- Official docs list macOS and Windows downloads, with Windows support in beta. [Desktop docs](https://docs.cline.bot/usage/cline-desktop)

### Key mechanisms

- **Workspace/session workflow:** select workspace and branch, choose provider/model, then start a session; separate sessions retain context for coding, research, documentation, or recurring jobs. [Desktop docs](https://docs.cline.bot/usage/cline-desktop)
- **Import and continue:** import supported Claude Code or Codex conversations from Settings; Cline carries prior context forward but continues with the provider/model configured in Cline. [Desktop docs](https://docs.cline.bot/usage/cline-desktop)
- **Scheduling:** build one-off or recurring tasks in the Schedule UI or ask Cline to create one from a conversation. Set name, frequency/date/time, prompt, provider/model/workspace, and optional system prompt, timeout, or tags. [Desktop docs](https://docs.cline.bot/usage/cline-desktop)
- **Research:** web search is enabled by default for new sessions; only providers with built-in search honor the toggle. Supported providers listed are Cline billing, ClinePass, OpenAI subscription, Anthropic, and Google Gemini, subject to model support. [Desktop docs](https://docs.cline.bot/usage/cline-desktop)
- **Capabilities/marketplace:** Installed inventory manages tools, plugins, skills, rules, MCP servers, and hooks; Marketplace browses/searches skills, MCP servers, and plugins by category. [Desktop docs](https://docs.cline.bot/usage/cline-desktop)
- **Remote and local environments:** SSH connects sessions to a dev box/build server/cloud VM; Ollama or LM Studio can run models locally, with memory-use indicator. [Desktop docs](https://docs.cline.bot/usage/cline-desktop)
- **Subagents vs. teams:** a subagent gets a separate prompt/context and returns a report to the main agent; several can run in parallel. Agent Teams use a coordinator, shared task board, mailbox, and mission log, persisted in `~/.cline/data/db/teams.db`. [Subagents](https://docs.cline.bot/features/subagents) · [Agent Teams](https://docs.cline.bot/cli/agent-teams)

### Concrete details

- Desktop schedule fields: name, frequency, date, time, prompt, provider, model, workspace, optional system prompt, timeout, tags. [Desktop docs](https://docs.cline.bot/usage/cline-desktop)
- CLI schedule commands documented separately include `cline schedule create`, `list`, `upcoming`, `active`, `trigger`, `pause`, `resume`, `update`, `history`, `stats`, `delete`, `export`, and `import`; common flags include `--cron`, `--prompt`, `--workspace`, `--provider`, `--model`, `--mode`, `--timeout`, `--max-parallel`, and `--delivery-adapter`/`--delivery-thread`. [Scheduling](https://docs.cline.bot/cli/scheduling)
- Team CLI entry point: `/team`; resume by session ID via `cline --id <session-id>`. Teams can be disabled through the `teams` tool; subagents through `spawn_agent`. [Agent Teams](https://docs.cline.bot/cli/agent-teams)
- Desktop subagent control is in Customize → Tools → `spawn_agent`; this setting applies to new sessions. [Subagents](https://docs.cline.bot/features/subagents)
- Desktop supports model choices via Cline provider (300+ models per docs), ClinePass, user API keys, supported local models, and imported Claude Code/Codex sessions. [Desktop docs](https://docs.cline.bot/usage/cline-desktop)

### Limits and weak points

- Windows is explicitly beta; the docs page names only macOS and Windows for Desktop downloads. [Desktop docs](https://docs.cline.bot/usage/cline-desktop)
- **Documentation scope conflict:** Desktop docs show a Schedule UI; the standalone Scheduling page says that scheduling applies to SDK, CLI, and Kanban, not VS Code/JetBrains, and does not mention Desktop. Treat exact parity/scheduling scope as unclear from these docs. [Desktop docs](https://docs.cline.bot/usage/cline-desktop) · [Scheduling](https://docs.cline.bot/cli/scheduling)
- Agent Teams are not documented as available in Desktop; the Teams page limits them to SDK, CLI, and Kanban. Desktop subagents are documented separately. [Agent Teams](https://docs.cline.bot/cli/agent-teams) · [Subagents](https://docs.cline.bot/features/subagents)
- Scheduling is powerful but background runs can approve their own tool calls because nobody is present; even Plan mode does not prevent all file writes through shell commands (the docs cite `python -c`). [Scheduling](https://docs.cline.bot/cli/scheduling)
- Web Search does not work merely because its toggle is on: the selected provider and model must support built-in search. [Desktop docs](https://docs.cline.bot/usage/cline-desktop)

## 4. ClinePass

Source: [Official ClinePass docs](https://docs.cline.bot/getting-started/clinepass).

### Summary (5 lines)

- ClinePass is an optional paid provider for selected open coding models, separate from Cline’s pay-as-you-go provider. [ClinePass docs](https://docs.cline.bot/getting-started/clinepass)
- The published price is $9.99/month. [ClinePass docs](https://docs.cline.bot/getting-started/clinepass)
- Cline advertises 2–5x the usage/quota versus standard API rates, but gives three time windows rather than numeric token/dollar allowances. [ClinePass docs](https://docs.cline.bot/getting-started/clinepass)
- The current model catalog includes DeepSeek, GLM, Kimi, Muse, MiniMax, MiMo, and Qwen models, with full IDs in the provider docs. [ClinePass docs](https://docs.cline.bot/getting-started/clinepass)
- It can also be called outside Cline through Cline’s OpenAI-compatible Chat Completions API using an API key. [ClinePass docs](https://docs.cline.bot/getting-started/clinepass)

### Key mechanisms

- Select ClinePass as the provider in the IDE extension settings or Cline CLI `/settings`; subscription is optional and can coexist with other providers. [ClinePass docs](https://docs.cline.bot/getting-started/clinepass)
- API use: `https://api.cline.bot/api/v1/chat/completions`; send the full `cline-pass/...` model ID and `X-Response-Format: openai`. Without that header, non-streaming responses nest completion under `data`; streaming is unaffected. [ClinePass docs](https://docs.cline.bot/getting-started/clinepass)
- Quota windows: rolling five hours, calendar week, and calendar month; current usage is shown on the Cline dashboard. [ClinePass docs](https://docs.cline.bot/getting-started/clinepass)

### Concrete details

- Price: `$9.99/month`. [ClinePass docs](https://docs.cline.bot/getting-started/clinepass)
- Current documented model IDs: `cline-pass/deepseek-v4.1-flash-1017`, `cline-pass/deepseek-v4.1-flash`, `cline-pass/deepseek-v4-pro`, `cline-pass/glm-5.3-flash`, `cline-pass/glm-5.3`, `cline-pass/kimi-k3.1-pro`, `cline-pass/kimi-k3`, `cline-pass/muse-spark-1.3-contributor`, `cline-pass/minimax-m3.1-flash`, `cline-pass/minimax-m3`, `cline-pass/mimo-v2.6-pro`, `cline-pass/mimo-v2.6-flash`, `cline-pass/qwen3.7-max`, `cline-pass/qwen3.7-plus`, `cline-pass/qwen3.8-max`. [ClinePass docs](https://docs.cline.bot/getting-started/clinepass)
- Reference pricing is per 1M tokens and is used to explain quota consumption, not charged separately; some models have peak/off-peak or context-length tiers. [ClinePass docs](https://docs.cline.bot/getting-started/clinepass)
- API credentials/config names: `CLINE_API_KEY`, model field `model`, HTTP header `X-Response-Format: openai`; endpoint `/api/v1/chat/completions`. [ClinePass docs](https://docs.cline.bot/getting-started/clinepass)
- Docs list removed models: GLM-5.2, Kimi K2.6, Kimi K2.7 Code, and DeepSeek V4 Flash, citing capacity constraints/model upgrades. [ClinePass docs](https://docs.cline.bot/getting-started/clinepass)

### Limits and weak points

- Exact five-hour, weekly, and monthly token/dollar/request caps are not published in the docs. “2–5x” is a comparison to standard API rates, not a precise allowance per user/model. [ClinePass docs](https://docs.cline.bot/getting-started/clinepass)
- The model roster is mutable; the docs explicitly announce removals and replacements. Recheck the official page before hard-coding model IDs. [ClinePass docs](https://docs.cline.bot/getting-started/clinepass)
- The cited underlying token rates are explanatory only; subscription use is not billed at those per-token prices. [ClinePass docs](https://docs.cline.bot/getting-started/clinepass)
- The page does not specify cancellation/refund terms, rollover, concurrency limits, or a guaranteed per-model quota. These details are **UNREAD / not stated in this source**. [ClinePass docs](https://docs.cline.bot/getting-started/clinepass)

## Ideas worth stealing for a desktop app that runs Claude Code and Codex CLIs on the user's own subscriptions

- **Keep the user-facing unit stable while letting tasks fan out:** present one ongoing project/assistant surface, but execute work in independent sessions with isolated context; use the installed Claude Code and Codex CLIs for actual runs. The Router/Worker separation is the clearest pattern. [DeepSeek article](https://xitter.cf/DeepSeekHarness/article/2116857257245987236) · [PandaCode bot architecture](https://github.com/aicodeking/pandacode/blob/main/docs/bots.md)
- **Add a small local task router before adding cloud infrastructure:** deterministic rules can route by project, source, user, or explicit mention; use a model only for ambiguous event triage. [DeepSeek article](https://xitter.cf/DeepSeekHarness/article/2116857257245987236)
- **Treat chat integrations as inputs to normal local task threads:** Telegram/Slack/Discord/WhatsApp messages should land in the same history, approval queue, and audit trail as desktop prompts. PandaCode shows concrete pairing and credential boundaries for this. [PandaCode bots docs](https://github.com/aicodeking/pandacode/blob/main/docs/bots.md)
- **Use delegation markers rather than hidden agent calls:** a `bot_message_bot(to, text)`-style handoff should show queued/running/done/failed state, target, and returned result in both conversations; cap recursion to prevent runaway chains. [PandaCode bots docs](https://github.com/aicodeking/pandacode/blob/main/docs/bots.md)
- **Make schedules ordinary prompts attached to a selected task/worktree/provider/model:** record next run, run history, pause, max runs, retries, and failure state. Keep local schedules honest about app-offline behavior. [PandaCode schedules](https://github.com/aicodeking/pandacode/blob/main/docs/schedules.md) · [Cline Desktop](https://docs.cline.bot/usage/cline-desktop)
- **Keep provider auth native:** invoke installed Claude Code/Codex binaries and let their existing CLI logins and subscription terms apply; store only app-specific settings in the desktop app. [PandaCode providers](https://github.com/aicodeking/pandacode/blob/main/docs/providers.md)
- **Separate policy from model judgment:** block/allow/ask rules, sender pairing, risk checks, approval UI, and a kill switch should be deterministic and local; never assume a bot personality prompt or semantic router is an authorization boundary. [PandaCode bots docs](https://github.com/aicodeking/pandacode/blob/main/docs/bots.md) · [DeepSeek article](https://xitter.cf/DeepSeekHarness/article/2116857257245987236)
- **Borrow session import and scheduled-work setup UX:** importing a Claude Code/Codex conversation and continuing it in a selected provider, plus creating recurring tasks in a form or by describing them conversationally, reduces setup friction. [Cline Desktop](https://docs.cline.bot/usage/cline-desktop)
- **Do not sell fixed model access if the product goal is BYO subscription:** ClinePass is a separate paid inference provider; a local CLI workspace can instead focus on orchestration, review, visibility, and bring-your-own CLI authentication. [ClinePass](https://docs.cline.bot/getting-started/clinepass) · [PandaCode README](https://github.com/aicodeking/pandacode/blob/main/README.md)
- **Keep the first version local and explicit about availability:** event watchers and schedules should show when the PC/app must remain running; persistent cloud control over local credentials/devices adds serious trust and prompt-injection risks. [PandaCode bots docs](https://github.com/aicodeking/pandacode/blob/main/docs/bots.md) · [DeepSeek article](https://xitter.cf/DeepSeekHarness/article/2116857257245987236)

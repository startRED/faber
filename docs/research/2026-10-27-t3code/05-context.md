# T3 Code study: context, composer, handoffs

Read-only study of T3 Code (MIT) at `t3code/`. Nothing was run, built or installed. Docs were read first; key claims were then checked by grep in `apps/server`, `apps/web`, `packages/contracts`, `packages/shared` and `packages/provider-core`.

Licensing note: T3 Code is MIT. Faber's AGENTS.md requires original code only, so every item below is an **idea** to re-implement. Anything copied verbatim would need the MIT notice kept and attribution added. None is recommended for copying.

## Verified claims (file paths)

| Claim | Evidence |
|---|---|
| Handoff strategies: `native_fork`, `portable_context`, `delta_context`, `fork_delta_context`, `checkpoint_context` | `packages/contracts/src/orchestrationV2.ts:153-169` |
| Context transfer statuses: `pending`, `resolved_native`, `resolved_portable`, `consumed` | `packages/contracts/src/orchestrationV2.ts:203-207` |
| Handoff summary is compacted to 240 chars by default | `apps/server/src/orchestration-v2/ContextHandoffService.ts:96` |
| Handoff summary capped at 32,000 chars | `apps/server/src/orchestration-v2/ContextHandoffService.ts:157` |
| Handoff token cap: default 16,000, env `T3CODE_CONTEXT_HANDOFF_TOKEN_CAP`, clamped to 1,024 to 64,000 | `packages/provider-core/src/server/handoffBudget.ts:9-18` |
| Token estimate is one token per UTF-8 byte, a pessimistic bound | `packages/provider-core/src/server/handoffBudget.ts:98,115` |
| Budget uses the provider's last reported usage, matched by native thread id and root node | `packages/provider-core/src/server/handoffBudget.ts` (`latestNativeContextUsage`) |
| Context reference links use `t3-context://v1/<kind>/<id>` inside Markdown links | `packages/shared/src/composerContextReferences.ts:8-20` (`CONTEXT_PROTOCOL`, `CONTEXT_LINK`) |
| Provider prompt envelope tag is `t3_context` | `packages/shared/src/composerContextReferences.ts:142` |
| Clipboard fragments carry the context as an encoded `data-t3-context-fragment` attribute | `packages/shared/src/composerContextClipboard.ts:28-38` |
| Provider projection of context happens at turn start and turn control | `apps/server/src/orchestration-v2/ProviderTurnStartService.ts`, `ProviderTurnControlService.ts` |
| Orchestrator MCP credential is a bearer token, revoked per provider session and per thread | `apps/server/src/mcp/McpSessionRegistry.ts:98-132, 182-183` |
| Orchestrator toolkit defines 17 tools (the doc says "eleven" and lists twelve; the code is the reference) | `apps/server/src/mcp/toolkits/orchestrator/tools.ts` |
| Orchestrator tools: `orchestrator_capabilities`, `delegate_task`, `task_status`, `task_cancel`, `t3_workflow_stop`, `t3_background_wait`, `schedule_task`, `list_scheduled_tasks`, `delete_scheduled_task`, `request_secret`, `create_threads`, `t3_thread_list`, `t3_thread_read`, `t3_thread_update`, `t3_thread_send`, `t3_thread_wait`, `t3_thread_interrupt` | same file |
| Other MCP toolkits: thread, project, worktree, preview, device, html, pull requests, attachment, environment | `apps/server/src/mcp/toolkits/*/tools.ts` |
| HTML preview tool with theme (dark/light) and console-level capture (log, info, warning, error) | `apps/server/src/mcp/toolkits/html/tools.ts:26-47` (`html_preview`) |
| Guest HTML render component | `apps/web/src/components/guest/GuestHtmlRender.tsx` |
| MCP Apps: server side under `apps/server/src/mcpApps/`, client frame and state in `apps/web` | `apps/web/src/components/chat/McpAppFrame.tsx`, `apps/web/src/state/mcpApps.ts`, `apps/server/src/mcpApps/*.ts` |
| Pooled usage limits per provider, with per-account windows, reset times and "Use reset" | `apps/web/src/components/usage/UsageLimitsPooled.tsx`, `UsageLimits.tsx`, `UsagePage.tsx` |
| Usage limits fetched per provider (Codex, Claude, Antigravity) and a recovery worker | `apps/server/src/provider/codexUsageLimits.ts`, `claudeUsageLimits.ts`, `antigravityUsageLimits.ts`, `apps/server/src/orchestration-v2/UsageLimitRecoveryWorker.ts` |

## Not verified (doc claims with no matching symbol or not yet grepped)

- `resolveStartContext` as named in the doc: no match in `apps/server/src`. The lazy resolution logic exists under other names (see `ContextTransfer` in `Orchestrator.ts`, `ProjectionStore.ts`, `ThreadForkService.ts`).
- Usage "5-minute minimum between automatic checks": not checked.
- Budget reserves (image 8,192, attachment 4,096) and 128,000 fallback window: not checked by name.
- "Copy MCP URL" and the `t3 thread` CLI: not found in `apps/web/src/components/auth/ConnectAgentSurface.tsx` by the grep attempted.
- Prompt links (`/new?prompt=...`), voice input and citations: taken from the docs only.
- Codex supports MCP Apps today: taken from the docs only.

## Ideas

### 1. Delta handoff when switching provider and back (resume the prior thread first)
- **What T3 does:** On a provider switch, it resumes the prior provider thread with a delta of what changed (`delta_context`, `fork_delta_context`). A full summary is only the fallback. The handoff is a budgeted textual record, not an agent-written summary. The budget is driven by the last usage the provider reported (`handoffBudget.ts`).
- **Why it helps Faber:** This is the core of moving a conversation between Claude Code and Codex and back. Resuming the native thread avoids re-sending the whole history, and the token budget keeps the handoff small.
- **Effort:** L (touches conversation storage, both CLI adapters and the budget).
- **Type:** Idea. Re-implement; do not copy.

### 2. Context references as typed records, not pasted text
- **What T3 does:** Attachments, threads and selections are stored as typed records. The composer shows a link `[label](t3-context://v1/<kind>/<id>)`. The text is only expanded when the provider is called. Clipboard copies carry the same record in an encoded attribute.
- **Why it helps Faber:** Files and past threads can be referenced in the chat without inlining them, so a handoff and a long conversation stay small and stay the same across providers.
- **Effort:** M.
- **Type:** Idea.

### 3. One context envelope per provider, with escaping
- **What T3 does:** Context is projected into a single `<t3_context version="1">` envelope at turn start, and each provider gets it in its own form (`ProviderTurnStartService.ts`).
- **Why it helps Faber:** A single, escaped envelope keeps Claude Code and Codex inputs the same, so one context works on both.
- **Effort:** S to M.
- **Type:** Idea.

### 4. Orchestrator MCP server: one agent drives other threads
- **What T3 does:** An MCP endpoint on the local server exposes about 17 tools (`delegate_task`, `task_status`, `task_cancel`, `create_threads`, `t3_thread_send`, `t3_thread_wait`, and more). Each provider session gets its own bearer token, which is revoked with the session and bound to a thread.
- **Why it helps Faber:** Lets Claude Code or Codex launch and watch other threads, which is the "orchestrator" use. The per-session token limits what a child agent can touch.
- **Effort:** L (new server endpoint, auth and tool surface). Start with 3 to 4 tools.
- **Type:** Idea.

### 5. Pooled usage and limits per provider
- **What T3 does:** Usage is read from each provider's own data (`codexUsageLimits.ts`, `claudeUsageLimits.ts`, `antigravityUsageLimits.ts`). The usage page pools accounts per provider with each window's reset time (`UsageLimitsPooled.tsx`).
- **Why it helps Faber:** A quota display for both Claude and Codex, which is what drives the switch decision in idea 1.
- **Effort:** M.
- **Type:** Idea.

### 6. Sandboxed HTML render in the chat
- **What T3 does:** An `html_preview` MCP tool renders HTML with a theme choice and captures console output at log, info, warning and error levels (`apps/server/src/mcp/toolkits/html/tools.ts`). The guest render is in `GuestHtmlRender.tsx`.
- **Why it helps Faber:** Lets an answer show a page or chart inline, with console errors visible to the agent.
- **Effort:** M. The sandbox and CSP rules need their own review.
- **Type:** Idea.

## Also seen (lower priority)

- **MCP Apps with consent** (`apps/server/src/mcpApps/`, `McpAppFrame.tsx`): interactive app frames inside a chat, gated by a consent prompt. Effort L. Idea.
- **Lazy context resolution at first dispatch** (`ContextTransfer` pending to consumed): context is resolved only when the next provider call happens, so a switch that is never used costs nothing. Effort M. Idea, part of idea 1.
- **Citations as plain message text** and **voice input that never submits**: from the docs only. Effort S. Idea.

## Open points for Erick

- The orchestrator doc counts "eleven" tools; the code defines 17. The code was treated as the source.

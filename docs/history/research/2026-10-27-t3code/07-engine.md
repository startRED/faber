# T3 Code to Faber: agent engine study

Source-only comparison, 2026-10-27. Repo A paths are relative to T3 Code; repo B paths are relative to `faber/`. No project scripts, builds, installs, live CLI sessions, or tests were run. Rankings combine user value and effort, not measured performance. A listed gap is based on the inspected source and product map, not a claim about every possible external CLI version. TypeScript/Effect implementation is not directly portable into Rust; protocol shapes and test fixtures can be translated, subject to retaining MIT notices for copied material.

## 1. Answer Codex questions in the conversation

- **T3:** Handles `item/tool/requestUserInput`, normalizes questions into user-input requests, and also supports asynchronous questions answered by a subsequent message (`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts:4090-4160,5465-5485`; `docs/internals/providers.md`, Protocol traps).
- **Faber:** Claude questions work, but the product map explicitly says Codex's equivalent is off (`docs/product-map.md`, Question card); Codex's approval handling focuses on command and file requests (`engine/src/codex.rs:1270-1285`).
- **Value / effort / port:** High / M / idea and protocol only. Unblocks the agent when it needs a choice instead of losing an interaction. Distinguish blocking RPC requests from asynchronous notifications and preserve native IDs.

## 2. Add safe per-turn checkpoints and undo

- **T3:** Captures and diffs Git-backed checkpoints (`apps/server/src/checkpointing/CheckpointStore.ts:80-126`); coordinates provider conversation rollback and file restore (`apps/server/src/orchestration-v2/CheckpointRollbackService.ts:142-175,222-276`); requires an isolated worktree for file restore (`apps/server/src/orchestration-v2/CheckpointRestoreSafety.ts:10-35`).
- **Faber:** Snapshots and undo are explicitly not built (`docs/product-map.md`, Features and their checks); mission worktrees exist but are not conversation checkpoints (`engine/src/project/`).
- **Value / effort / port:** High / L / idea only. Gives a reliable way back after agent edits. Restore must preserve unrelated work and align the provider's history with restored files; disable file restore where isolation cannot be proved. Claude's `resumeSessionAt` and Codex rollback support are provider-specific, not interchangeable.

## 3. Keep a capability manifest separate from the model list

- **T3:** Models adapter capabilities for resume, rollback, approval, planning, subagents and other operations; orchestration selects behavior based on those flags (`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts:232-292`; `docs/orchestration-v2/provider-capability-system.md:1-35,197-236`). Provider guidance requires only natively enforced modes to be offered (`docs/internals/adding-a-provider.md`, Report only what the provider does). Catalog metadata is merged with live models (`packages/provider-core/src/server/ModelCatalog.ts:64-98,120-122`).
- **Faber:** Has five modes and explicit mappings (`engine/src/permissions.rs`, `engine/src/codex.rs:169-213`); Codex queries `model/list` (`engine/src/codex.rs:1593-1599`), but routing profiles are compiled in (`engine/src/route.rs:13-32,66-70`) and the live manifest is planned (`docs/product-map.md`, Features and their checks).
- **Value / effort / port:** High / M / idea and data schema only. Prevents offering unavailable combinations and lets new model options arrive without pretending a static profile proves account access. Keep native CLI discovery authoritative for actual availability; version any separately maintained classification.

## 4. Respect permission mode support and native approval semantics

- **T3:** Maps Codex policies including native auto-review (`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts:688-720`); handles distinct command, file, and permissions approval requests (`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts:5030-5206`). Its provider rules say unsupported modes fall back to supervised rather than simulate provider enforcement (`docs/internals/adding-a-provider.md`, Report only what the provider does).
- **Faber:** Has Ask, Accept edits, Plan, Auto and Full access, with Auto explicitly falling back to Accept edits when unavailable (`docs/product-map.md`, Words and settings); handles command/file approvals (`engine/src/codex.rs:1270-1285`) and tracks stale grants (`engine/src/engine/answer.rs:59-96`, `engine/src/model.rs`). The inspected dispatch did not establish handling of Codex's separate permissions approval request.
- **Value / effort / port:** High / M / protocol and policy idea. Audit that request and preserve provider-native decisions, scope and IDs; never silently widen a permission. Do not replace Faber's existing conservative fallback without verifying installed CLI support.

## 5. Preserve Codex plan artifacts and structured progress

- **T3:** Normalizes streamed plan text and structured `turn/plan/updated` steps into a plan artifact (`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts:3790-3888,4209-4252`), independently of permission mode.
- **Faber:** Has Plan permission mode (`engine/src/permissions.rs`; `docs/product-map.md`, Words) and a work log, but the inspected `engine/src/codex.rs` has no handler for `turn/plan/updated` or `item/plan/delta`.
- **Value / effort / port:** High / M / idea and protocol only. Makes the agent's proposed plan and step changes visible and durable. Keep plan interaction mode distinct from file/command authorization.

## 6. Track what a returning provider has already seen

- **T3:** Records handoff strategy and coverage; can select a delta since the target last saw the thread or a full handoff, and budgets intact history items (`apps/server/src/orchestration-v2/ContextHandoffService.ts:216-232,422-478`; `packages/provider-core/src/server/handoffBudget.ts:89-119,214-224`). It marks historical material as context and explicitly notes omitted native tool state.
- **Faber:** Already switches providers and reconstructs history; long histories can be summarized (`engine/src/engine.rs:181-213`; `engine/src/handover.rs:45-90`). This is an improvement to an existing feature, not a missing handoff.
- **Value / effort / port:** High / M / idea only. Coverage per native session can avoid repeating the whole conversation when switching back and can disclose missing context. Validate behavior for failures, branches, attachments and a lost native session before using a delta.

## 7. Preserve accepted work across crashes with durable commands

- **T3:** Commits normalized events, projection changes, command receipts and side-effect records transactionally; serialized publication preserves sequence (`apps/server/src/orchestration-v2/EventSink.ts:257-277,358-380,625-706`; `apps/server/src/orchestration-v2/EffectOutbox.ts:201,380-415`; `apps/server/src/orchestration-v2/EffectWorker.ts:726-782`). Receipts make repeated commands identifiable; the outbox claims work with leases.
- **Faber:** Conversation JSON is replaced via a temporary file (`engine/src/store.rs:52-86,157-190`), and Codex records a tentative session on turn acceptance (`engine/src/engine.rs:1376-1407`). Mission state has its own journal/effects (`docs/product-map.md`, Other stored state). The inspected chat path does not provide a transactional event log and durable outbox.
- **Value / effort / port:** High / L / idea only. Can reduce ambiguous crash windows between saving a request and starting a CLI. This is not exactly-once execution: external CLI calls need explicit idempotency or reconciliation, and a full event-sourced rewrite is not justified without demonstrated failure cases.

## 8. Batch streaming updates without changing their order

- **T3:** Coalesces text, reasoning and plan deltas; reserves item positions before delayed flush and flushes terminal turns (`apps/server/src/orchestration-v2/Adapters/providerTextDeltaCoalescer.ts:42-48,92-148`; `apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts:3202-3263,3862-3888,5853-5855`). It opts out of Codex's full diff notifications and derives diffs from checkpoints (`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts:210-232`; `docs/internals/providers.md`, Provider diagnostics).
- **Faber:** Already optimizes streaming display by rendering Markdown block by block and holds UI events together during games (`docs/product-map.md`, Chat conversation and Features and their checks). Engine-side equivalence was not established from this source pass.
- **Value / effort / port:** Medium / S–M / idea only. Measure event rate, CPU and latency first; batch only if warranted, preserve tool/text ordering, and flush before completion or interruption. Opting out of diff updates is useful only if another complete diff source exists.

## 9. Recover native sessions at a precise conversation head

- **T3:** Claude can resume at a particular native message, detects metadata-only transcripts that cannot be resumed, and can fall back to a fresh session/handoff (`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts:1406-1496,9724-9734,10489-10615`). It marks a native session opened only after success (`:9782-9786`).
- **Faber:** Already uses Claude `--resume` and Codex `thread/resume` (`engine/src/claude.rs:303-321`; `engine/src/codex.rs:791-796`), keeps only actually used session IDs, and has lost-session history fallback (`engine/src/engine.rs:173-184,212-214,1376-1407`).
- **Value / effort / port:** Medium / M / idea only. Precise native heads become useful for branch/undo and avoid resuming unsaved placeholders. Verify the installed Claude protocol exposes the equivalent before implementing; Faber's existing resume is real and should remain the default.

## 10. Handle stop races and settle every open item

- **T3:** Serializes Codex completion against interrupt timeouts and explicitly settles incomplete tool calls (`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts:1806-1813,2142-2151`); Claude bounds interrupt, then closes and settles child work (`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts:10203-10269`). Its provider guide calls out Stop arriving before native turn-start (`docs/internals/adding-a-provider.md`, Traps several providers hit).
- **Faber:** Sends native interrupt for both CLIs and has a Claude interrupt timeout (`engine/src/codex.rs:1045-1046,1464-1486`; `engine/src/claude.rs:613-618,811-822`). Faber also detects apparently stuck subagents (`engine/src/stuck.rs`, `engine/src/engine/stuck_watch.rs`).
- **Value / effort / port:** Medium / M / idea and test scenarios. Check early Stop, completion racing with Stop, pending approvals and process exit using recorded frames. These are edge cases to verify, not a finding that Faber lacks timeouts or process cleanup.

## 11. Keep background and resumed subagent identity across turns

- **T3:** Reconstructs resumed Claude subagents from native task/session references and keeps session-level mappings (`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts:5476-5559,6020-6106`). Its provider guidance says child updates must be routed by child identity, not merely whichever parent run is active (`docs/internals/adding-a-provider.md`, Traps several providers hit).
- **Faber:** Already models Claude and Codex subagents, their status, details, approvals and stuck detection; it also offers cross-vendor helpers (`engine/src/claude.rs`, `engine/src/codex.rs:292-368`, `engine/src/helper.rs`; `docs/product-map.md`, Subagent step and Helper from the other AI).
- **Value / effort / port:** Medium / M / idea only. Audit late background results, resumed children and terminal states across subsequent turns. Do not assume T3's graph can replace Faber's existing richer helper UX.

## 12. Add ACP as a separate provider transport when expansion is wanted

- **T3:** Implements ACP initialization and capability negotiation, chooses load/resume only when advertised, creates sessions, prompts and cancels (`packages/provider-acp/src/server/AcpSessionRuntime.ts:2051-2065,2176-2211,2356-2356,2578-2613`; `packages/effect-acp/src/client.ts:928-938`). Shared ACP adapter and registry allow more agents without treating them as Claude or Codex (`docs/internals/adding-a-provider.md`, Choose the integration shape).
- **Faber:** Has dedicated Claude stream-json and Codex app-server drivers (`engine/src/claude.rs:1,202-204`; `engine/src/codex.rs:791-796,1027`); no ACP driver found in the inspected engine.
- **Value / effort / port:** Medium / L / protocol and architecture only. Broadens provider choice, but brings authentication, permission, terminal, filesystem and lifecycle differences; do not assume every ACP agent implements resume or every mode. Low priority if Faber remains intentionally limited to two providers.

## 13. Make protocol recovery and diagnostics bounded and useful

- **T3:** Uses request timeouts in adapters; native diagnostic logging filters high-volume token frames and caps encoded payloads, while preserving failure/routing details (`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts:1677-1697,2913-2919`; `packages/provider-core/src/server/nativeProtocolLogging.ts`; `docs/internals/providers.md`, Provider diagnostics). Claude distinguishes terminal aborts from superficially successful envelopes (`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts:3334-3408`).
- **Faber:** Has CLI/watchdog handling including interrupt timeouts and quota-specific handling (`engine/src/claude.rs:613-618`; `engine/src/codex.rs:529-577`; `engine/src/engine.rs:904-959`); cache-health logging is size-bounded (`engine/src/store.rs:319-326`). Comparable bounded raw-protocol diagnostics were not established.
- **Value / effort / port:** Medium / M / idea only. A redacted, opt-in and bounded trace makes protocol changes diagnosable without retaining entire prompts or high-volume chunks; never treat a local completion frame alone as proof an upstream retry recovered (`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts:1865`).

## 14. Reuse real-provider transcripts for lifecycle regression tests

- **T3:** Replays captured native frames through the real adapter, orchestration, persistence and projections, replacing only transport; covers interrupt, resume, approvals and subagent/checkpoint lifecycles (`docs/orchestration-v2/testing-strategy.md:15-25,94-105,135-144,163-179`).
- **Faber:** Already has Claude/Codex fixtures and session, resume and permission tests (`engine/tests/claude_session.rs`, `engine/tests/codex_session.rs`, `engine/tests/session_resume.rs`; `tests/fixtures/claude/`, `tests/fixtures/codex/`; `docs/product-map.md`, Features and their checks).
- **Value / effort / port:** Medium / M / scenarios and sanitized fixture format. Extend existing tests with captured versioned native transcripts and end-to-end state assertions, especially for race conditions; do not assume existing fixtures are synthetic without inspecting their provenance.

## Coverage notes: existing behavior, not standalone gaps

- **Launch/protocol:** T3 uses Claude Agent SDK `query()` with streaming, resume and permission callback (`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts:997-1000,1449-1496`); Faber intentionally drives a long-lived Claude `-p` stream-json process (`engine/src/claude.rs:1,202-204,303-321`). Both speak Codex app-server with initialize, native thread resume and interrupt (`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts:1697,2913-2919,2452-2466`; `engine/src/codex.rs:791-796,1027,1045-1046`). An SDK migration alone is not a demonstrated improvement; it is a Node dependency and would require preserving Rust process ownership and feature parity.
- **Usage and quota:** T3 merges Codex rate-limit notifications (`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts:4262-4284`); Faber reads token totals, quota updates and explicit quota reads (`engine/src/codex.rs:529-577,1528-1547`) and maintains a quota ledger (`engine/src/engine.rs:904-918`, `engine/src/ledger.rs`). Faber also has pacing and paid-use safeguards. No broad quota deficiency was demonstrated.
- **Tools and diffs:** Both normalize tool execution. Faber already counts changed lines and presents file changes (`docs/product-map.md`, Work log); T3 has structured plan/tool items and checkpoint diffs (`apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts:3533,3790-3888`; `apps/server/src/checkpointing/CheckpointStore.ts:121-126`). A checkpoint diff covers the final workspace, while a tool event reports an individual action; neither automatically replaces the other.
- **Persistence scope:** T3's execution graph and SQLite architecture are substantially broader than a desktop conversation file. Faber's separate mission journal is not evidence that chat has the same transactional properties, and neither architecture alone proves actual user-visible data loss or superior performance.

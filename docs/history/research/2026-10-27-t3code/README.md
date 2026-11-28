# T3 Code study

T3 Code at commit `36d4d711`, studied on 2026-10-27, offers useful protocol and performance practices for Faber, but much of its product already overlaps with Faber or conflicts with its rules. The best immediate work is to close specific Codex interaction gaps and measure expensive conversation updates; safe undo and large-history work belong later. These are source-based judgments, not measured performance results. Paths below are relative to the named repository. Faber's UI lives in `ui/`, not `ui/src/`; both `engine/src/` and the actual UI tree were searched.

## Take now

Ranked by expected user value, with S = small, M = medium and L = large. A missing handler means none was found in the inspected source, not proof about every CLI version. All implementations below are idea only.

1. **Answer Codex questions in the existing card.** Handle blocking user-input requests and asynchronous questions with their native IDs, and return answers or skips correctly. T3: `apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts`. Faber: `engine/src/codex.rs` has no handler for `item/tool/requestUserInput` (a local change in progress turns the tool off instead); `ui/Ask.tsx` and `engine/src/engine/answer.rs` already handle Claude questions; R160 requires questions to reach the screen. Why: a Codex agent can ask for a choice without losing the interaction. Effort: M. Port: idea only. Area: engine, Chat and Projects.
2. **Handle Codex permission requests without widening access.** Audit the separate permissions approval request and handle it only when supported by the installed protocol, preserving request IDs, scopes and conservative fallback. T3: `apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts`, `docs/internals/adding-a-provider.md`. Faber: `engine/src/codex.rs` handles command and file approvals; `engine/src/permissions.rs` and R140 already define modes. Why: an unrecognized request must not silently grant broader rights or leave a turn stuck. Effort: M. Port: idea only. Area: engine and existing approval card.
3. **Keep Codex plans and their progress.** Capture native plan updates and show them in the existing conversation, separate from permission decisions. T3: `apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts`, `apps/web/src/components/chat/ProposedPlanCard.tsx`. Faber: `engine/src/codex.rs` has no `turn/plan/updated` or `item/plan/delta` handler found; Plan permission mode exists in `engine/src/permissions.rs`; R8 requires a plan before big work. Why: the plan and its changing steps remain visible rather than disappearing into protocol events. Effort: M. Port: idea only. Area: engine and conversation UI.
4. **Make provider handoffs incremental when returning.** Record which conversation head each native session has seen; send only the missing context when a verified resume succeeds, otherwise use the existing full handoff. T3: `apps/server/src/orchestration-v2/ContextHandoffService.ts`, `packages/provider-core/src/server/handoffBudget.ts`. Faber: `engine/src/engine.rs` and `engine/src/handover.rs` already reconstruct history; this is a refinement, not a new handoff. Why: switching back can avoid resending old context; test branches, attachments and lost sessions first. Effort: M for a bounded first slice; broader context budgeting can follow. Port: idea only. Area: engine, storage, both providers.
5. **Reduce oversized conversation updates after measuring them.** Measure event sizes and render work with long replies and noisy tools; if needed, send bounded changes and fetch detail on demand while preserving ordering, errors and the full provider context. T3: `docs/internals/performance-regressions.md`, `apps/server/src/orchestration-v2/Adapters/providerTextDeltaCoalescer.ts`. Faber: `engine/src/engine.rs` already batches streaming at about 30 updates per second; `ui/Thread.tsx` uses comparison memos. Why: repeated whole-message payloads can waste memory and CPU, but savings are not yet measured. Effort: M. Port: idea only. Area: engine-to-UI events and conversation UI.
6. **Keep work groups readable at interruptions.** Extend the planned grouped work phrases so questions, approvals, failures and changes of direction stay distinct. T3: `apps/web/src/components/chat/MessagesTimeline.logic.ts`. Faber: `ui/worklog.ts`, `ui/Thread.tsx`; `docs/product-map.md` says grouped phrases are planned. Why: a busy work log is easier to follow without hiding something needing the person. Effort: M. Port: idea only. Area: Chat and Projects work log.
7. **Add an opt-in, bounded protocol trace.** Keep short, redacted diagnostics for protocol failures, with strict size and retention limits; never collect credentials or full prompts by default. T3: `packages/provider-core/src/server/nativeProtocolLogging.ts`, `docs/internals/providers.md`. Faber: `engine/src/codex.rs` and `engine/src/claude.rs` have error and timeout handling; `engine/src/store.rs` bounds cache-health logs, but an equivalent raw-protocol trace was not established. Why: CLI changes become easier to diagnose without unbounded logs. Effort: M. Port: idea only. Area: engine diagnostics and Privacy.

## Take later

1. **Safe per-turn snapshots and undo.** Capture a private snapshot, preview changes, protect unrelated edits and align provider history before allowing restore. T3: `apps/server/src/checkpointing/CheckpointStore.ts`, `apps/server/src/orchestration-v2/CheckpointRestoreSafety.ts`, `CheckpointRollbackService.ts`. Faber: R8 and `docs/product/areas.md` already require undo; `docs/product-map.md` says snapshots and undo are not built, while `engine/src/project/` has mission state and copies. Why: completes an existing promise; do not treat mission checkpoints as conversation snapshots. Effort: L. Port: idea only. Area: engine, project copies and conversation UI.
2. **Bound loaded conversation history.** Page before loading and decoding, retain pending actions and scroll anchors, and virtualize visible rows only if measurement warrants it. T3: `docs/internals/performance-regressions.md`, `apps/web/src/components/chat/MessagesTimeline.tsx`. Faber: `ui/App.tsx` and `ui/Thread.tsx` initially draw 40 messages but open a conversation snapshot. Why: protect the R125 memory budget in very long conversations. Effort: L. Port: idea only. Area: conversation storage, engine and UI.
3. **Inspect changed files on demand.** Show a bounded, plain-language view of each changed file, loading details only when opened. T3: `apps/web/src/components/DiffPanel.tsx`, `apps/web/src/lib/diffRendering.ts`. Faber: `ui/worklog.ts` and R188 show counts and a few lines; a full viewer was not found. Why: users can inspect work without a heavy always-open view; respect R188's pane direction. Effort: L, dependent on the existing pane and safe snapshot design. Port: idea only. Area: Projects and conversation UI.
4. **Track native session heads for safe rollback.** Resume from a verified native message when supported, falling back to Faber's existing history handoff. T3: `apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts`, `CheckpointRollbackService.ts`. Faber: `engine/src/claude.rs`, `engine/src/codex.rs`, `engine/src/engine.rs` already resume and recover sessions. Why: prevents a restored file state from being paired with the wrong conversation head. Effort: M–L, depends on undo and provider support. Port: idea only. Area: engine and both providers.
5. **Make accepted chat work recoverable after a crash.** First reproduce the failure window; then persist only the minimal intent and reconcile it with the CLI before retrying. T3: `apps/server/src/orchestration-v2/EventSink.ts`, `EffectOutbox.ts`, `EffectWorker.ts`. Faber: `engine/src/store.rs` atomically replaces conversation files; mission journaling is separate. Why: reduce ambiguous saved-but-not-started work without claiming exactly-once execution or rewriting the engine. Effort: L. Port: idea only. Area: chat persistence and engine.

## Needs the owner

Existing decisions stay in force until changed; these questions concern unresolved scope or implementation choices, not permission to undo them.

- **Phone access:** For the R13/R14 Phase 7 remote control, should the first version work only on the local network, or also through a user-managed private network? A hosted paid relay is excluded. T3: `apps/server/src/cli/pair.ts`, `apps/server/src/cloud/`; Faber: `docs/product/README.md`, R13–R14.
- **Guest links:** Should anyone be able to share a read-only conversation through an expiring, revocable link, or should sharing remain a manual export? This adds privacy and access risk. T3: `apps/server/src/guest/`, `apps/web/src/components/guest/GuestPage.tsx`; Faber: R35 keeps conversation history private, and no guest feature was found in `engine/src/` or `ui/`.
- **Routines:** For Phase 7, should scheduled tasks run only while the PC and Faber are available, or run missed tasks at the next launch? R148 already decides next-launch catch-up for the owner's separate scout, not every product routine. T3: `apps/server/src/scheduledTasks/`; Faber: R13, R45, R148 and `docs/product/README.md`.
- **Project file references:** Should people choose a file from a small `@` menu, or use the existing attachment flow and plain paths? T3: `apps/web/src/components/chat/ComposerCommandMenu.tsx`; Faber: `ui/Composer.tsx` has attachments, but a project-file mention picker was not found. This is a visible interaction choice, not a reason to replace the textarea.
- **Embedded terminal:** Should an interactive terminal live inside Faber, or should the existing outside terminal remain the only option? T3: `docs/internals/terminal-runtime.md`; Faber: `ui/Thread.tsx`, `src-tauri/src/terminal.rs`. An embedded terminal costs memory and needs a separate decision; automatic console windows remain forbidden by C006.

## Engineering process

These extend existing Faber checks; they are not claims that Faber lacks a test process.

1. **Test a packaged upgrade with old data.** Launch the actual release artifact with an isolated old profile and verify migration and startup. T3: `scripts/smoke-desktop-app.ts`. Faber: `testkit/` and `docs/product/distribution.md` cover existing tests and releases; a matching packaged old-profile smoke was not established. Keep updates and telemetry disabled in the fixture.
2. **Replay recorded provider lifecycle cases.** Extend `engine/tests/claude_session.rs`, `engine/tests/codex_session.rs` and `tests/fixtures/` with sanitized frames for early stop, completion races, pending questions and resumed children. T3: `docs/orchestration-v2/testing-strategy.md`. Existing Faber fixtures are real infrastructure, not a missing feature.
3. **Measure actual conversation scenarios.** Add render/event-size scenarios for long history, typing, streaming and noisy tools alongside the existing whole-app memory test. T3: `scripts/render-metrics.ts`, `docs/internals/performance-regressions.md`. Faber: `testkit/src/screen/app.rs`, R125 and R191. Measurements decide whether optimizations are needed.
4. **Evaluate model-facing text when it changes.** Add small repeatable cases for instructions and tool descriptions, with a failing case for a reproduced regression. T3: `evals/README.md`. Faber: `docs/product/README.md` already plans benchmarks and `testkit/coherence/` tests agent coherence; extend those rather than creating a competing evaluation system.
5. **Test update failure and restart against a local feed.** Exercise interrupted installs and stable/preview isolation without publishing a release. T3: `apps/desktop/src/updates/DesktopUpdates.ts`, `apps/desktop/src/updates/updateChannels.ts`. Faber: `src-tauri/src/update.rs`, `src-tauri/src/local_update.rs`, R156; local-build rollback exists, but an equivalent published-installer recovery record was not established.

## Dropped

- **Basic handoff, resume, streaming batching, stable Markdown and scroll following:** already in `engine/src/engine.rs`, `engine/src/handover.rs`, `engine/src/claude.rs`, `engine/src/codex.rs`, `ui/markdown.tsx` and `ui/App.tsx`; only specific improvements above remain.
- **Quota display, pooled usage and notifications:** `engine/src/ledger.rs`, `ui/Sidebar.tsx`, R92 and R177 already cover the relevant product behavior.
- **Provider expansion and ACP:** `docs/product/README.md` limits Faber to Claude Code and Codex; no new provider transport is justified.
- **Separate live capability manifest:** already planned in `docs/product/architecture.md` and `docs/product/models.md`; build the existing plan rather than duplicate it.
- **HTML visual answers, full-text search, grouped work and undo as new product directions:** already planned in `docs/product-map.md`, `docs/product/following.md` and R8; only concrete implementation practices appear above.
- **Update channels, checksum verification, affected tests and memory gate:** R156 and `src-tauri/src/update.rs`, `engine/src/install.rs`, R165 and `testkit/`, and R125 already cover them. The packaged-upgrade test above fills a narrower unverified gap.
- **Always-on telemetry sampler, foreground focus workaround and hidden-first window:** no demonstrated need; R125, R191 and C006 prioritize low idle cost and no focus theft.
- **New help page, rich editor, prompt links, custom keybindings and a separate settings search:** no established need; R18 directs help to Faber, `ui/Composer.tsx` already provides a textarea, and `ui/Settings.tsx` has sections.

## Do not copy

- **Windows `taskkill` process-tree cleanup:** T3's Windows path can open a console; Faber owns processes through `engine/src/proc.rs` and `winproc/src/lib.rs`, and C006 forbids visible consoles or stolen focus.
- **Destructive checkpoint restore:** T3's restore can discard workspace changes. R83 forbids deleting unsaved work; any Faber undo must protect unrelated edits and prove isolation.
- **Hosted relay, public tunnel or paid service:** conflicts with the no-paid-extras constraint; a remote-control design must work without Faber operating a paid relay.
- **Browser credential or session import:** Faber never reads CLI credentials and does not copy browser logins; see `AGENTS.md` and `docs/product/README.md`.
- **One-click branch, push and pull-request controls or hidden agent-credit removal:** R83 gives Faber explicit project guardrails; non-programmers should not see git or PR terminology, and authorship should not be silently altered.
- **Electron, React components, extra provider SDKs or a full event-sourcing rewrite:** Faber uses Tauri, Solid and Rust, and its memory budget and existing architecture require a demonstrated need for any extra machinery.

## Licensing

T3 Code is MIT licensed. The owner allowed taking its code (2026-10-27): when a small pure function is copied or
adapted, keep T3's copyright line and the full MIT permission notice in a header of that file, and check the licenses of
anything it depends on. AGENTS.md's "Original code only" rule names TL-ADE, TL-Control and TL-Orchestrator; its first
words read wider, so a change that copies T3 code should also make that rule say MIT code with its notice is allowed.
Most items above are ideas anyway: T3 is TypeScript on Effect and React, Faber is Rust and Solid.

## Sources

Each report below was written by one reader and checked only where this page cites it; treat line numbers as pointers.

| Page | What it covers | Reader |
|---|---|---|
| [01-user-features.md](01-user-features.md) | Catalog of T3's 41 user pages with source paths | Haiku |
| [02-desktop.md](02-desktop.md) | Electron shell, updates, resource sampling, smoke tests | Haiku |
| [03-remote.md](03-remote.md) | Phone and remote access: pairing, auth, relay, costs, a minimal Faber version | Haiku |
| [04-source-control.md](04-source-control.md) | Worktrees, per-turn checkpoints, diffs, cleanup, commit and PR flow | Haiku |
| [05-context.md](05-context.md) | Context references, provider handoffs, orchestrator MCP, usage, HTML renders | Haiku |
| [06-engineering.md](06-engineering.md) | Agent guides, lint rules, affected tests, evals, perf budgets, releases | Haiku |
| [07-engine.md](07-engine.md) | How T3 drives Claude Code and Codex compared with Faber's engine | Codex |
| [08-screen.md](08-screen.md) | Timeline, long threads, composer and performance compared with Faber's screen | Codex |

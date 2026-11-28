# T3 Code → Faber: read-only UI study

Scope: source and documentation inspection only. No project scripts, builds, installations, browser sessions, runtime measurements, or modifications to either repository. Ranks are judgment calls on value per effort for Faber's 400–500 MB whole-app budget. Repo A paths are relative to T3 Code; Repo B paths are relative to Faber. Line numbers refer to the inspected revisions. “Missing” means not found in the inspected screen/product map, not proof that no backend capability exists. MIT attribution applies to copied T3 code, subject to its license and any third-party dependency licenses; React components are design ideas, not drop-in Solid components. Memory and speed benefits require measurement in Faber.

## 1. Bound loaded history, not just visible messages

- **T3:** Recent history is paged before decoding, targeting roughly 1 MiB, 10 user turns/500 rows for initial opens and 20 turns for older pages, with special smaller windows and documented exceptions; preserves scroll on older-page merges (`docs/internals/performance-regressions.md:13-28`).
- **Faber:** Opens a conversation snapshot, initially draws the last 40, then decreases `from` as the reader moves up (`ui/App.tsx:68-69,326-337,491-499`; `ui/Thread.tsx:468-476`). Already loaded data and previously revealed rows are not bounded by this mechanism.
- **Value:** High: limits memory and open-time growth on very long conversations. **Effort:** L. **Portability:** Algorithm idea/pure TS with attribution where applicable; Faber also needs changes in its Rust data boundary. Retain pending requests and the visible anchor; measure whole-app memory, not JSON bytes.

## 2. Keep command output out of routine updates

- **T3:** Omits raw command, dynamic-tool result and inline file-change bodies from normal wire events; subscribes to a bounded, throttled command tail only while expanded (`docs/internals/performance-regressions.md:33-48`).
- **Faber:** Engine sends the whole streaming message every 33 ms; value-comparing memos reduce downstream redraws (`ui/Thread.tsx:112-130`); work-log steps exist (`ui/worklog.ts`). Transport differs, so this is a direction, not a directly comparable wire measurement.
- **Value:** High: fewer repeated large payloads and allocations, especially during noisy commands. **Effort:** M. **Portability:** Architecture/algorithm idea; implement at Faber's engine/UI event boundary, preserving failure state, on-demand output and any provider context.

## 3. Measure rendering and transfer regressions on real scenarios

- **T3:** `scripts/render-metrics.ts:8-26,36-80` runs and compares scenario render counts and checks nondeterminism; `apps/web/test/renderCounter.ts:39` and `apps/web/test/renderScenarios.renders.tsx:1-18` instrument actual React components. Wire fixtures exercise v2 reducers, explicit byte/row budgets and a 900-row turn (`docs/internals/performance-regressions.md:3-12,59-68`). These are not RSS or compressed-wire measurements.
- **Faber:** Already has a whole-app memory gate and UX sentinel (`docs/product-map.md`, Features and their checks; `testkit/src/screen/app.rs`, `testkit/src/sentinel.rs`). A Solid render-count equivalent was not found in this inspection.
- **Value:** High: makes a performance fix provable and catches regressions before they reach users. **Effort:** M. **Portability:** Measurement idea only for React instrumentation; adapt to Solid and preserve Faber's actual process-memory gate. Include long threads, streaming, opening older history, typing and noisy tools; compare repeat runs and memory separately.

## 4. Group consecutive work without hiding interruptions

- **T3:** Groups work by run, handles steering boundaries, avoids folding question answers as ordinary work, and offers stable expandable group rows (`apps/web/src/components/chat/MessagesTimeline.logic.ts:1029-1188,1752-1847`). Active work is grouped separately (`:1488-1542`).
- **Faber:** One folded answer work log with steps, commands and helper rows (`ui/Thread.tsx:232-266`; `ui/worklog.ts`); product map explicitly says grouped phrases such as “Changing 3 files” are planned (`docs/product-map.md`, Work log).
- **Value:** High: reduces noise while preserving an understandable timeline. **Effort:** M. **Portability:** Pure grouping algorithm may be adapted with attribution, but T3's provider/run schema is different; Solid display is idea only. Preserve approvals, questions, errors and steer boundaries as separate visible events.

## 5. Keep scroll position through streaming and expansion

- **T3:** Tracks following the end separately from remembered per-thread positions; disclosure changes suspend end maintenance, and rows/scroll anchors preserve context (`apps/web/src/components/chat/MessagesTimeline.tsx:384-401,509-516,895-941,1284-1312`; `MessagesTimeline.logic.ts:320-336`).
- **Faber:** Stops following when the user scrolls up, re-arms near the bottom and offsets prepended content (`ui/App.tsx:484-499`).
- **Value:** High: protects reading position when a work group opens, a message grows or the user switches threads. **Effort:** M. **Portability:** Behavior and pure anchor logic can be adapted; UI integration is Solid-specific. First reproduce an actual jump in Faber; its current follow behavior already covers the basic case.

## 6. Virtualize long histories after measuring the existing window

- **T3:** LegendList estimates row size at 90 px, renders list items with maintained visible position, tracks recycled rows and memoizes row content (`apps/web/src/components/chat/MessagesTimeline.tsx:806-815,888-895,1212-1215,1284-1312,1396-1469`).
- **Faber:** Initially mounts 40 and appends older batches on demand, but does not unmount them as the reader continues upward (`ui/App.tsx:68-69,491-499`; `ui/Thread.tsx:468-476`).
- **Value:** High for extremely long threads: bounded DOM and less layout work. **Effort:** L. **Portability:** Virtualization and scroll-anchor algorithms only; LegendList and React components are not drop-in. Variable-height streaming rows, text selection, focus and accessibility require careful handling. Do this only if measurements show Faber's current progressive rendering is insufficient.

## 7. Add a lightweight file mention picker

- **T3:** Composer command menu distinguishes workspace paths, skills and slash commands, with search/loading/empty states (`apps/web/src/components/chat/ComposerCommandMenu.tsx:40-55,144-152,243-245`; `packages/client-runtime/src/state/composerPathSearch.ts:3-15`).
- **Faber:** Textarea and file attachment picker exist (`ui/Composer.tsx:62-73,190-230`); a project `@file` picker was not found in the inspected composer.
- **Value:** High for coding: users can point to a project file without copying a path. **Effort:** M. **Portability:** Search ranking and parsing are pure-TS candidates with attribution; menu is idea only. Start with bounded file-name search and keyboard navigation; avoid loading a rich editor solely for this.

## 8. Make changed files inspectable on demand

- **T3:** A file tree, per-file collapse and lazy patch loading separate the summary from full diff content (`apps/web/src/components/DiffPanel.tsx:364-430,456-529,992-1023,1077-1130`). Diff content has per-file cache identity (`apps/web/src/lib/diffRendering.ts:214-248`).
- **Faber:** Counts changed lines and extracts a few changed lines in the work log (`ui/worklog.ts:259-301`; `ui/Thread.tsx:201-205`); a full in-app diff viewer was not found in the product map.
- **Value:** High for project work: the user can inspect what changed without leaving the conversation. **Effort:** L. **Portability:** UI idea; per-file cache/key logic is adaptable with attribution. Start with a plain, on-demand unified diff and explicit truncation; avoid importing the whole Pierre/worker stack until measured.

## 9. Avoid rebuilding stable streaming Markdown

- **T3:** Keeps row activity state separate from shared row context and memoizes row content (`apps/web/src/components/chat/MessagesTimeline.tsx:674-699,1396-1469`).
- **Faber:** Already retains unchanged Markdown DOM blocks and reparses only changed trailing blocks; HTML and changed link definitions deliberately cause wider redraws (`ui/markdown.tsx:43-75`). Independent memos compare streaming fields by value (`ui/Thread.tsx:112-130`).
- **Value:** Medium: protect an existing strong implementation rather than replace it. **Effort:** S for targeted measurement. **Portability:** Idea only; React memo patterns do not transfer directly. Verify lexer cost on very long single blocks and selection preservation before optimizing.

## 10. Offer slash commands without inventing provider behavior

- **T3:** Filters provider slash commands to the start of a prompt, ranks commands and skills against a normalized query (`apps/web/src/components/chat/composerSlashCommandSearch.ts:15-23,26-40,71-100`); the menu distinguishes local and provider commands (`ComposerCommandMenu.tsx:40-55`).
- **Faber:** Plain text entry in `ui/Composer.tsx:190-230`; slash discovery was not found in the inspected UI.
- **Value:** Medium: makes available agent actions discoverable, but only if Faber exposes provider-supported commands. **Effort:** M. **Portability:** Pure ranking/search code can be adapted with attribution; menu is idea only. Never advertise a command a chosen provider cannot execute.

## 11. Use contextual attachment references only when needed

- **T3:** Records hold attachment identity separately from inline occurrences, not bytes; labels are not identity, and removing a file's last chip releases its upload (`docs/internals/composer-context-references.md:11-26,150-167`). Missing records have an unresolved representation (`:102-107`).
- **Faber:** Attaches images/files, and embeds small text files in message text (`ui/Composer.tsx:47-73,164-187`).
- **Value:** Medium: placing a reference beside the exact instruction can help multi-file prompts; a full typed context protocol is costly. **Effort:** L. **Portability:** Data model/codec ideas and pure TS may be adapted with attribution; editor chips are idea only. Avoid duplicating base64 data and keep a plain text fallback.

## 12. Prefer the textarea until rich editing earns its cost

- **T3:** Rich editor extensions change with settings; controlled content changes must not replace editor contents for cursor-only changes (`docs/internals/composer-editors.md:1-17`). T3 also has a Tiptap editor (`apps/web/src/components/ComposerPromptEditorTiptap.tsx`).
- **Faber:** Uses a focused textarea with attachment handling; size measurement is limited to at most once per frame (`ui/Composer.tsx:83-89,190-223`).
- **Value:** Medium: avoiding unnecessary editor state and dependencies supports the memory budget and stable typing. **Effort:** S to retain current approach; L for a rich editor. **Portability:** Principle only. Preserve cursor, undo, IME composition and accessibility when adding mentions.

## 13. Plans, approvals and questions should be distinct actions

- **T3:** A proposed plan has its own card (`apps/web/src/components/chat/ProposedPlanCard.tsx:1-22,131-135`); pending approvals and user input have separate composer panels (`ComposerPendingApprovalPanel.tsx:1-77`; `ComposerPendingUserInputPanel.tsx`). Timeline emits a proposed-plan row (`MessagesTimeline.tsx:1451-1469`).
- **Faber:** Permission and held-back cards plus question cards already exist (`ui/Ask.tsx`; `ui/Thread.tsx`; `docs/product-map.md`, Question card); provider coverage differs and Codex's equivalent is documented as off. A dedicated plan card was not established in this inspection.
- **Value:** Medium: a plan can be understood and acted on without mixing it with tool noise. **Effort:** M. **Portability:** Components are idea only; preserve Faber's existing approval authority and honest provider limitations. Do not add duplicate approval surfaces.

## 14. An embedded terminal is a separate, expensive product choice

- **T3:** Server terminal history is incremental and capped at 5,000 lines/8 MiB; client buffer is capped at 512 KiB. A shared WASM instance hosts terminal handles, with rendering outside React and replay isolated from the live PTY (`docs/internals/terminal-runtime.md:10-44`).
- **Faber:** Opens a visible external console for a conversation (`ui/Thread.tsx:320-333`; `docs/product-map.md`, Outside windows); no embedded terminal was found in the product map.
- **Value:** Medium for users needing interactive shell work inside Faber; low if external terminal already suffices. **Effort:** L. **Portability:** Retention, replay safety and lifecycle ideas only; Ghostty WASM, fonts and renderer have their own footprint and license/dependency review. Measure total memory with active and hidden terminals before adopting.

## 15. Limit diff work per file and per viewport

- **T3:** Virtualized diff line layout avoids split-column reflow (`apps/web/src/components/diffs/StyledDiffCodeView.tsx:16-17,111-115`); a shared worker pool performs highlighting and waits for initialization (`apps/web/src/components/DiffWorkerPoolProvider.tsx:50-60,89-108`); shows explicit incomplete-preview notice (`apps/web/src/components/DiffPanel.tsx:992-998`).
- **Faber:** Only work-log diff snippets were identified (`ui/worklog.ts:259-301`), not a full viewer.
- **Value:** Medium, conditional on building the viewer: large patches should not freeze the app. **Effort:** L. **Portability:** Layout/virtualization ideas; React/Pierre components are idea only. First benchmark plain text and lazy file loading, because workers, syntax grammars and caches consume memory too.

## 16. Reuse row identity and fetch details only when visible

- **T3:** Initial shell holds active navigation rows only; archive and transcript bodies are separate queries (`docs/internals/performance-regressions.md:49-52`). Palette results lease live repository queries only for visible rows plus overscan (`:105-110`).
- **Faber:** Sidebar groups and keys chat rows, retaining unchanged DOM during refresh (`ui/Sidebar.tsx:128,280-285`); title/project search is already present (`docs/product-map.md`, Sidebar).
- **Value:** Medium for a large sidebar: prevent background detail work from growing with all conversations. **Effort:** M. **Portability:** Query-lifetime and stable identity ideas; adapt to Solid signals. First inspect and measure Faber's data retrieval before adding virtualization.

## 17. Keyboard shortcuts: focus and collision handling before customization

- **T3:** Keybinding settings search, capture shortcuts, cancel recording with Escape and flag conflicts (`apps/web/src/components/settings/KeybindingsSettings.tsx:115-138,717-749,793-835`). Page-scroll logic considers whether the editor itself can scroll (`apps/web/src/components/chat/pageScrollController.ts:18-21,37-68`).
- **Faber:** Existing Ctrl+N and Ctrl+. shortcuts, focused composer and focus-restoring menus/overlays are documented (`docs/product-map.md`, Sidebar and Topbar; `docs/design/README.md`, Rules for every screen; `ui/Sidebar.tsx:20-34`). A customizable shortcut editor was not found.
- **Value:** Medium: scoped handling prevents stealing keys from text fields and dialogs; full customization is less compelling. **Effort:** S for a targeted focus fix, L for editable bindings. **Portability:** Pure key parsing logic can be adapted with attribution; React settings controls are idea only. Don't add a settings panel without demand.

## 18. Settings search should reveal the exact control

- **T3:** Has settings search/navigation, a sectioned sidebar and searchable keybindings (`apps/web/src/components/settings/settingsSearch.ts`; `SettingsSidebarNav.tsx`; `KeybindingsSettings.tsx:115-138,1268-1281`).
- **Faber:** Settings has sections and resets scroll to top when switching (`ui/Settings.tsx:19,98-103`); no settings search was established in the inspected files.
- **Value:** Low to medium: useful as settings grow, but not urgent for a short menu. **Effort:** M. **Portability:** Search and navigation ideas; components are idea only. Keep Faber's one-short-line settings design and translations.

## 19. Empty states and perceived speed: keep Faber's established design

- **T3:** Provides draft-hero headline and project heading (`apps/web/src/components/chat/DraftHeroHeadline.tsx`, `DraftHeroProjectHeading.tsx`) and optimistic user messages (`apps/web/src/components/chat/optimisticUserMessages.ts`); loading and empty messages distinguish searching from no results (`ComposerCommandMenu.tsx:144-152`).
- **Faber:** Already has a focused home, suggestions, first-run, no-AI state and a static first frame before scripts (`ui/Welcome.tsx`, `ui/FirstRun.tsx`, `ui/Empty.tsx`; `docs/design/README.md`, First frame and update progress). Existing product-map notes identify project/chat inconsistencies.
- **Value:** Low for wholesale visual borrowing; medium for honest optimistic feedback and actionable empty search states. **Effort:** S–M. **Portability:** Visual/interaction ideas only. Preserve Faber's identity and reduced-motion rules; verify send failure/rollback before changing optimistic behavior.

## 20. Small details that make existing surfaces feel immediate

- **T3:** Avoids rewriting minimap styles when strips do not change and remeasures lane width on actual width changes, not every streamed frame (`apps/web/src/components/chat/MessagesTimeline.tsx:854-870,973-1000`); shows copy only for completed, nonempty output (`MessagesTimeline.logic.ts:696-703`).
- **Faber:** Already limits composer measurement to a frame, preserves sidebar rows, and keeps Markdown selection/stable blocks (`ui/Composer.tsx:89`; `ui/Sidebar.tsx:280-285`; `ui/markdown.tsx:59-75`).
- **Value:** Medium when a profile identifies a hot path, otherwise low. **Effort:** S. **Portability:** Techniques, not React components. Measure layout and DOM writes; avoid adding animation or a second copy control to imitate T3.

## Recommended sequence and limits

Start with measurement and the full-message/tool-output path, then grouped work and scroll correctness; add bounded history and virtualization if the long-thread scenario actually breaches the budget. File mentions and a small on-demand diff are the highest-value new coding interactions. Keep the external terminal and textarea until a concrete workflow justifies their memory cost. The ordered list above ranks expected value per effort, not an implementation dependency graph. Findings are static inspection only: no claimed measured speedup, memory saving, complete feature inventory, or runtime proof.

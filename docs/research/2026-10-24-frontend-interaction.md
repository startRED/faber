# Frontend interaction research

Status: Research complete. This is a dated design proposal and validation plan; no frontend change or user study was completed.

## Scope and current Faber context

This study focuses on a desktop work app: clear task status, multiple conversations and nested subagents, visible progress, opening a child session, the running column, the embedded browser, Settings and Catalog. It covers UX (whether people can finish their task), UI (visual hierarchy and legibility), IxD (controls, feedback and recovery), and research methods. It does not claim that any product is best or that Faber is easier to use.

The current product documents already specify child steps under agent work, rows that open a child conversation in the running column, and fact-based completion details. Extend those paths and their shared status model; do not add a competing agent board. [Following work](../product/following.md#following-the-work) is a proposal where marked, not proof that every item is implemented. The screen's existing cursor rule is deliberate: arrow cursor on controls and rows, text cursor on selectable content, pointer only on text links. The W3C cursor page is a Working Draft and defines `pointer` as indicating a link, so this study does not recommend changing that rule by itself.

## Evidence and method

### Standards and design guidance

- [WCAG 2.2](https://www.w3.org/TR/WCAG22/) provides testable accessibility criteria. For this desktop WebView, review keyboard focus visibility and obstruction, text contrast, target size, pointer cancellation, and alternatives to dragging. WCAG combines automated checks with human evaluation; a green automated result is not conformance.
- [Fluent 2 accessibility](https://fluent2.microsoft.design/accessibility), [layout](https://fluent2.microsoft.design/layout), and [design tokens](https://fluent2.microsoft.design/design-tokens) are practical desktop-app references for keyboard/focus, semantic hierarchy, spacing by proximity, reflow and shared semantic tokens. Fluent says its components target WCAG 2.1 AA; this is guidance, not evidence about Faber.
- [Nielsen Norman Group's ten heuristics](https://www.nngroup.com/articles/ten-usability-heuristics/) supply broad review prompts: show current system status, use familiar language, preserve control, prevent/recover from errors, and keep useful information recognizable. [A heuristic evaluation](https://www.nngroup.com/articles/how-to-conduct-a-heuristic-evaluation/) identifies likely issues through expert inspection. A heuristic is not a user finding, and violating one is not automatically a defect.
- The [GOV.UK user research guide](https://www.gov.uk/service-manual/user-research) distinguishes methods by the question being asked. [Moderated usability testing](https://www.gov.uk/service-manual/user-research/using-moderated-usability-testing) observes representative people attempting believable tasks without being shown the answer; [research planning](https://www.gov.uk/service-manual/user-research/plan-user-research-for-your-service) says to set questions, audiences and objectives first. A small qualitative round can reveal usability problems; it does not establish statistical success.
- [W3C CSS UI Level 4](https://www.w3.org/TR/css-ui-4/#cursor) describes cursor meanings. It is a Working Draft. Keep semantic button/link behavior and keyboard access independent from cursor styling.

### Product comparison, clean behavioral notes

Only public product documentation and static package observations were used. The notes below describe visible capabilities in plain language; they do not reproduce source code, internal identifiers, prompts or private conversation content.

| Product and evidence | Observed/documented interaction pattern | Limit |
| :--- | :--- | :--- |
| [Antigravity subagents](https://www.antigravity.google/docs/subagents/), [artifacts](https://www.antigravity.google/docs/artifacts), and [Teamwork](https://www.antigravity.google/docs/teamwork/) | A parent can start concurrent child sessions; the app docs say the child can be opened from the subagent panel. Child lifecycle includes running, idle and killed; running work can be stopped. Work can be summarized in plans, diffs, recordings and milestone/progress artifacts. The published [changelog](https://www.antigravity.google/changelog) also describes dedicated subagent cards, live state and one-click navigation to the child conversation. | Changelog and docs cover multiple product surfaces and releases; the installed interface was not opened, so exact placement and current app rendering remain unverified. |
| [ChatGPT Desktop transition](https://help.openai.com/en/articles/20001276-moving-to-the-new-chatgpt-desktop-app), [ChatGPT Work and Codex](https://help.openai.com/en/articles/20001275), [Codex app](https://openai.com/index/introducing-the-codex-app/), and [multi-agent guide](https://developers.openai.com/api/docs/guides/agents-api/multi-agent) | Current desktop documentation groups Chat and Work in Recents with sorting, filtering and pinning; Codex stays a separate view/history. Codex supports concurrent agent work in threads organized by project. The multi-agent guide describes separate contexts coordinated by a main agent. Work task documentation describes reviewing progress, answering questions, changing direction and approving consequential actions. | The public docs do not establish that each Codex child transcript has the same one-click full-detail UI as Antigravity. Do not assume it does. |
| [Claude Code subagents](https://code.claude.com/docs/en/sub-agents) and [Anthropic's autonomy update](https://www.anthropic.com/news/enabling-claude-code-to-work-more-autonomously) | Claude Code documents focused subagents and parallel work; Anthropic distinguishes subagents from long-running background processes. | The bounded official search found no equivalent documented nested-session monitor for general Claude Desktop chat. This does not prove the feature is absent. |

## Design direction for Faber

Keep the main conversation as the parent. Render each delegation at its originating step and in the existing running column, using one consistent child row/card that names the work in a short task phrase; show the agent role as secondary context. The visible label should explain the task, not expose an opaque session ID.

Show lifecycle in plain words: queued, working, needs you, done, failed, or stopped. Include the latest observable step, elapsed time, and a concise result or artifact when available. Do not invent percentage completion. A compact parent summary may count active, waiting-on-person and completed work, with the waiting item first; clicking a child opens its complete user-visible transcript and tool results in the existing side pane while preserving the parent position. Keep stop/message controls scoped to capabilities the CLI actually supports, and label when a message goes to the parent instead of a busy child.

Preserve the existing model in which progress comes from recorded events and the child's request/result, rather than asking the model to grade its own work. Do not expose hidden chain-of-thought. Keep artifacts, errors, approvals and user-visible tool output reviewable. Long transcripts and streaming output should have bounded rendering and stable scroll behavior; an open child should not pull the parent away from the user's reading position.

Make every state understandable without color alone. Use the current amber attention and vendor colors only according to the design system. Use semantic controls, visible keyboard focus, focus restoration after opening/closing details, and localized strings in every supported language. Keep reduced-motion behavior, high-contrast/Windows modes, long translations, empty/offline/first-run states, and the native browser pane's overlay boundary in scope. Do not place a new setting on a work screen; use the established placement rules if a preference becomes necessary.

## Task-based validation plan (not run)

1. **Baseline and scope:** inspect Chat, Projects, Settings and Catalog as a newcomer and as a frequent user; record what is implemented versus only documented. Include no CLI, no active work, empty history, a queued message, streaming, a question/approval, completion, error and stopped work.
2. **Controls and keyboard:** check every button, link, menu, selectable row, disabled control and text field for its real action, enabled/disabled explanation, pointer affordance, accessible name and keyboard operation. Tab/Shift+Tab, Enter/Space, Escape, shortcuts and focus return must match the screen's existing rules.
3. **Nested sessions:** with synthetic parent/child/grandchild work, start, expand, collapse, open, return, message, stop, fail and reopen completed work. Confirm labels remain distinguishable, counts reconcile, state transitions are legible, errors/requests draw attention once, and the full child history never loses the parent reading position.
4. **Progress, streaming and browser:** stream long text and frequent tool events while scrolling; test latest-message behavior, side-pane switching, back/close, browser overlays, dialogs, menus, toast placement and scroll ownership. Verify the browser pane cannot be covered by page content and that its controls keep keyboard access.
5. **Responsive and visual checks:** repeat representative tasks at 720, 1280 and 1920 px. At each size check clipping, wrapping, hierarchy, dense lists and long names. Repeat with reduced motion, Windows contrast/high-contrast settings, light/dark themes, longest locale, 200% zoom where supported, and keyboard-only input.
6. **Heuristic review:** have a reviewer who did not implement the screen record issue, location, affected task, severity and proposed fix against the NN/G heuristics. Treat this as expert inspection only.
7. **Real user research:** after a concrete prototype exists, run moderated task sessions with current or likely Faber users, including keyboard/assistive-technology users. Ask them to find a child, understand what it is doing, inspect the full transcript, interrupt or redirect when supported, then return to the parent. Record success, wrong turns, recovery, hesitation and participant words. No sessions were conducted for this report.

## Evidence log and limits

Date: 2026-10-24. Research model: GPT-6. Static artifact analysis: REA Agents 6.4.0 / SDK 2.3.1 through the Codex MCP client 0.162.1. No apps were launched and no user profiles, credentials, tokens, conversation transcripts or personal app data were read. The implementation request includes comparison screenshots; the static artifact analysis did not inspect their personal conversation content. No files outside this report and `docs/README.md` were changed.

| Installed artifact | Version and SHA-256 | REA evidence and coverage |
| :--- | :--- | :--- |
| Antigravity ASAR | 2.26.0; `8ce27ff7c2defa10ed1ed2817288a61c546690f4cae1b1a8c91d4e69e0851e4d` | Summary `ev_17c56c48acd40bd0db2680f774ecfe8ed3e3a61adc3f2c2e5eef99a2d6067956`, parent `ev_53f236571beabda4b4cb199915f33ae38499058d4edb9bcb067f7958cc0e3326`. 766 relevant files, 666 JS files parsed, no parse failures. Semantic coverage partial/truncated by result budget. |
| Claude ASAR | 3.6556.2; `3e9cb8e8034e75d9e1996f960169877c556e17a454041a25233b372493fec9ac` | Summary `ev_413c1f052927a1773e6ffe85a2fb6a1c8e736bf23a928e30d6c35c3738beee48`, parent `ev_d8cf6998418a8b7ea854e1e2ac5dac2fcbf124c316bf6583025d55b5906a376f`. 573 relevant files, 556 JS parsed, no parse failures. One unpacked native companion hash contradicted the archive declaration; rerun used `record-and-continue`, explicitly treating that file as untrusted. Semantic coverage partial/truncated by heap/result limits. |
| ChatGPT Desktop ASAR | 26.1007.2314.0; `C5FDEF328CFA2AD9095786F9C3A7AAA447E68834D31299D6EF2592DA02029038` | Installed package manifest identifies the app as ChatGPT; ASAR is 567,399,024 bytes. REA analysis was not run on this large package. Product claims above rely on official public documentation, not package inspection. |

Static analysis does not prove runtime reachability or visible layout. Official docs are product claims, not independent usability studies. This report contains no measured user outcomes, implementation, accessibility conformance result, or evidence that Faber is better than the compared apps. Re-check current docs and installed versions before implementation; validate the Faber flows above before calling the redesign complete.

## Implementation validation (2026-10-24)

The first implementation slice preserves compatible Markdown nodes during streaming, enlarges compact help/switch
hit areas, and uses native checkboxes in forced colours. A second slice shows the latest observed unfinished tool
under a running helper task; between steps it retains the last observed result without collapsing the row. These extend the existing conversation, controls and helper view; they add no dependency.

- Negative controls reproduced paragraph replacement, undersized compact hit areas, and the missing helper activity.
  The corrected streaming check also covers selection, code focus/scroll, changed reference links, removal of unsafe
  attributes, shrinking content, and answer HTML that imitates a copy-button class.
- The combined hidden-Chrome run executed 30 checks: all passed normally and with forced colours. An independent
  read-only reviewer checked the implementation and the three synthetic helper-row captures. These are component
  checks, not native application or screen-reader certification.
- A separate 80-check browser/composer/settings/shortcut/shell audit initially passed 75. Five failures involved
  focus, viewport assumptions, simulated drops or mock storage. Matching CSS viewport sizes and replaying actual
  mouse input through CDP passed the two suspected focus regressions (720 px navigation and 1920 px helper expansion).
  The Settings storage and simulated-drop failures are not diagnosed application bugs; native confirmation remains open.
- The 720, 1280 and 1920 px synthetic streaming fixture kept the same paragraph, text node, growing list and code
  block. The reading anchor and scroll offset were unchanged while updates arrived below the viewport. The code
  block also survived closing its fence. Each size used 200 updates per format (paragraph, list and code).
- Measured JavaScript setter totals at 720 px were 412 ms, 1,195 ms and 1,189 ms for the three phases; at 1920 px,
  436 ms, 1,265 ms and 1,216 ms. These totals cover 200 updates, not one frame or perceived interaction latency.
  The retained JavaScript heap after collection grew about 0.95–1.05 MB as the fixture content grew. This is not
  native application memory, isolated overhead, an FPS result or a before/after performance comparison.

Temporary receipts and captures are under the ignored `target/frontend-proof/` folder: `current-combined*.json`,
`audit/`, `review/` and `stream-visual/`. Permanent regression checks live in `tests/ui/thread.check.ts`,
`grammar.check.ts` and `agents.check.ts`. The full local proof pass is recorded in
`.faber/verify/1792875062143-full.log`; the first queue submission does not by itself prove integration or a release.

Still open: the live native-browser exit, nested helper navigation, complete native control coverage, long-language
and screen-reader walks, and whole-app resource measurements. A DOM exit clone cannot carry a native browser surface;
changing only its opacity is not sufficient evidence that browser closing is fixed. No claim of outperforming the
compared applications follows from this validation.

### Native task walk

The existing first-user harness rebuilt the hidden native test app at 1280 x 800 in Portuguese, with an isolated
profile and a tool-less image reviewer. The initial 19-capture walk reached the end of first run, Projects and the
language setting. Catalog was visible, but the reviewer returned no valid action; its generated claim that navigation
was unclear is not a confirmed UI defect. Connecting from the no-AI fixture failed at the deliberately dead loopback
setup source, not a production service. No real sign-in or installer was exercised.

A focused Demo-project replay completed in nine actions: project entry, new conversation, model choice, typed question,
send, visible waiting state, then the recorded assistant response and the explicit "Nothing running" state. The first
eight-action attempt stopped before completion; a bounded sixteen-action follow-up captured the actual outcome.
Evidence: `target/frontend-proof/first-user-project-complete-native.md` and the final capture in
`target/faber-test/first-user/2026-10-24-2FdFHD/`. Both runs closed the isolated app cleanly. These are AI-guided fixture
walks, not sessions with human participants, native browser exit coverage, every-button coverage or resource benchmarks.
The latest helper-between-steps correction was made after the initial build; its proof remains component-level.

### Coordination failure still open

Cross-conversation coordination was refused with the same generic message for more than one target. Source inspection
shows that `engine/src/engine/siblings.rs` replaces every `send_by_itself` failure with one fallback, so the actual
cause cannot be inferred from the tool result. Busy delivery normally queues successfully; queue saturation, admission
and dispatch errors remain distinct possibilities. The existing tests cover successful busy delivery, not retention
of a send refusal reason. This source is claimed by another active lane; no production change was made here.

### Sampled themes, languages and keyboard interaction

The isolated component fixture exercised Dark Room, Morning and High contrast with the five shipped languages at
720 CSS px and an emulated 360 CSS px viewport. The second size is an out-of-minimum stress case, not proof of native
200% zoom: Faber sets a 720 x 480 logical minimum. Tab/Space activation passed in the 30 fixture combinations; Help
measured 28 x 28 and the switch 32 x 28. At 720 px, the sampled controls and helper row had no horizontal overflow.
Sampled helper metadata/activity contrast was 5.46:1, 5.26:1 and 12.2:1 in those three themes. These measurements do not
cover every colour, component, custom theme or accessibility requirement.

In the 360 px stress case, Settings retained its 288 px navigation and clipped content; this is not established as a
supported-window defect or a native-zoom result. Forced-colour emulation kept the checked native checkbox visible.
The fixture's vertical below-fold flags and its derived overflow counters are not reliable failure summaries; inspect
the individual geometry and keyboard records in `target/frontend-proof/stream-visual/stress-matrix.json` instead.

The follow-up component checks also keep helper-row height stable between tools, keep result words before any clipped
detail, and align the Catalog switch with its alternative action button. All 31 combined checks passed normally and
with forced colours; an independent reviewer reran the affected 22 checks in both modes. Negative receipts are
`agent-between-steps-before.json`, `agent-result-clipped-before.json` and `catalog-centre-before.json` under the same
temporary evidence folder.

### Nested navigation follow-up

- `nested-before.json` reproduces nested tasks being folded into the root task; `nested-navigation-before.json`
  reproduces the missing child navigation control. `nested-background-before.json` catches a child incorrectly
  reading stopped when its ancestor continues in the background after the reply ends.
- The current component suite has 32 passing checks in normal and forced-color modes. Nested tasks own their
  direct steps, the parent keeps the delegation button, and Back returns to the parent task. The same existing
  reader route and lazy words API are used; this does not extend the 16,000-character retention limit.
- General validation initially stopped at a build-cache reservation with no owner receipt. The three shared
  reservations were preserved; only this lane's waiting wrapper was stopped. An isolated, normally leased cache
  allowed all seven full checks to pass (`1792878938444-full.log`). Newer nested changes are checked separately.
- Independent review found the delegated-task button's wrapping rule was overridden by the later shared button
  rule. `nested-long-title-before.json` reproduces it; the permanent 300 px geometry check passes after the more
  specific rule. Independent synthetic captures at 720, 1280 and 1920 px confirm 396 px client/content width,
  normal wrapping and no lateral clipping in the inspected panel.

### Follow-up: retained history and native browser evidence

The later retained-history change supersedes the earlier 16,000-character preview limit for new calls. Legacy calls
keep their existing preview; new visible text and tool results are retained in atomic pages, read eight pages at a time.
The hidden native fixture navigated parent and child histories, older/later/latest pages, full-thread view and restart.
The successful run retained 17 synthetic photographs; a fresh independent review inspected them alongside nine baseline
photographs and found no visual regression. The largest sampled app tree was 219.8 MiB, including WebView2. These
samples do not establish a continuous peak or real-user research.

The real native browser exposed two additional interface defects: load completion could leave Back/Forward stale,
and a failed native open called a valid address unsupported. Component checks reproduce both before the change,
including a delayed older history response; 32 checks pass after it at narrow/wide sizes, forced colors and reduced motion.
A settled native close produced 18 distinct observed child positions with column opacity remaining 1, confirming travel
without a fade in that fixture. Separate native checks passed real navigation, physical window bounds, native
minimization and synthetic localStorage separation across conversations while preserving the host marker. The browser
fixture sampled 236.6 MiB during use and 237.0 MiB minimized; five minimized seconds used 281 ms of process-tree CPU.
The offscreen document still reported visible, so this is not evidence of normal hidden-page behavior or zero idle cost.
The parent/child retained-page walk also passed in Projects. No comparison establishes superiority over another product.

### Current navigation and recovery follow-up

Later hidden native runs reproduce and fix a narrow-window native-page disappearance during closing, and an accepted
navigation lost on fast tab changes. The corrected browser closes opaquely and restores the right page at 720 and
1920 px, with 13 observed intermediate native positions at each width. The tab walk passes switching, reopening,
middle-click, loaded overflow selection, close-others and last-tab cleanup. Real retained-history Retry recovers a corrupt
synthetic page in the same open parent reader. Running/finished-card and native window control walks also pass.

The latest browser fixture samples up to 233.4 MiB across three app-tree observations. Minimized it samples 231.1 MiB
and 172 ms CPU over five seconds. Its 6,984 ms CPU over 5.5 seconds of active probing is not an idle or frame-rate
benchmark. The skipped theme-transition readiness correction is integrated and its component check passes. A later
complete UI run passed 733 checks and exposed one remaining pane-reopening failure; the targeted negative control
then failed on the stale width-glide flag and passed after the visibility reset. The complete rerun after that reset
passes all 734 checks, with no failures, unrun or skipped checks (`ui-final-glide-run.log`, 205 seconds including
preparation). Closing during an already active legitimate view-width transition is not separately established.
After resolving the real conflict with the newer base, preserving both exit guards and current select choices, the
combined source passes 743 UI checks with no failures, unrun or skipped checks (`ui-base-integrated-run.log`, 260.5
seconds including preparation). The source and documentation conflict resolutions received an independent review.

The native helper Send/Continue walks pass at emulated 720 and 1280 px: keyboard Send reaches the recorded helper,
its distinct response arrives once, and Continue preserves and focuses the existing main draft. These walks exposed
one real history defect: process metadata split a short response into 120 pages. The replay fails before grouping text
at content boundaries and passes afterward; both native widths pass with the readable reply kept together. Four
retained native photographs received an independent visual review. Temporary test-profile paths were shortened after
an isolated IndexedDB backing-store error; the same draft check then passed and all 14 related full-suite failures
cleared. The sandbox's four audited boundaries, canaries, environment and cleanup remain unchanged; its eight unit
checks pass.

Native screen-reader operation and participant research remain unverified. These receipts establish the named
synthetic interactions, not every possible button sequence or superiority over other products.

### Integrated native close and reopen follow-up (2026-10-25)

The first exact integration run passes 2,296 Rust and 743 UI checks, but fails eight of 144 native scenarios
(`landing-integrated-console.log`, `screen-integration-red.log`); nothing was queued. Reopening during a retained
exit canceled its animation but returned before restoring modal state and focus. The narrow correction preserves the
existing open path for an interrupted close. The other failures sampled cleanup or layout after accessibility hiding
rather than after actual removal; resource checks now await zero live panes before counting frames or patch lines.
An independent read-only review found no static blockers (C011 checked). The focused native repeat passes seven
of the eight scenarios, including modal focus and resource removal. The remaining visual-version scenario advances
to its later warmed memory check, then fails: 155.3 MB versus a 145.0 MB baseline and a 5 MB margin. Retired frame
references are empty and its extra renderer exits; these facts do not explain the remaining working-set increase.
The full native suite is not yet passing (`screen-pane-reopen-7-of-8.log`).

A diagnostic repeat retains the original failing comparison: closing at 144.4 MB, then resizing alone raises
the closed app to 149.9 MB before any reopen; the final closed reading is 154.0 MB. The 5.5 MB resize cost makes
the original widths incomparable (`visual-retention-diagnostic.log`, 0/1). The corrected check uses the closed
720 px reading as its baseline, preserves the strict 5 MB margin, collected frame references and process checks,
and includes both resized readings in the 400 MB cap. A fresh independent review finds no blockers (C006/C009);
the corrected runtime result remains pending. The 4.1 MB difference is diagnostic evidence, not a green rerun.

The corrected same-width native repeat passes 1/1 (`visual-retention-matched-green.log`, 69.6 seconds including
preparation): the final closed app is 153.3 MB versus a 150.3 MB closed baseline, a 3.0 MB difference within
the unchanged 5 MB margin; frame references are empty and the retired renderer exits. Together with the preceding
7/8 correction run, all eight formerly failing scenarios now have focused passing evidence. This is not yet a
passing full integration rerun. Full local commands pass six checks in verifier receipt 1792907343983-full; its
shared-cache claims attempt was interrupted before a compiler child and is not a pass. The same claims command
passes separately in claims-pane-final.log through the normally leased isolated cache (exit 0, 27.7 seconds).

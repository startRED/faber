# Frontend blueprint evidence audit, 2026-10-22

Static source/document inspection only: no app, test, performance run, assistive technology or competitor comparison
was executed for this audit. A cited check is existing coverage, not a fresh pass. Input: the person's 243-ID blueprint.
Snapshot: the updated frontend-excellence workspace based on origin/phase-1-base, not the older conversation checkout.

Existing means direct narrow source/coverage evidence; partial means the foundation exists but the whole criterion is
unproved or incomplete; missing identifies a gap in the inspected flow or living parity inventory, not a repository-wide
absence proof; unknown means this inspection cannot establish status. No out_of_scope label is assigned without a
product non-goal. Keep means extend the existing path; measure means obtain evidence before changing; defer avoids
speculative/product-scale work. Decisions are recommendations, not implementation claims. No item was fixed by this audit.
50/100 ms, INP, AA conformance, zero loss and current RAM/CPU remain unmeasured here.

[Part 2: T01–F16](2026-10-22-frontend-blueprint-audit-part-2.md).
The findings and row statuses below preserve the initial inspection. Subsequent implementation, executed checks and
integration state are recorded in the [implementation record](../superpowers/plans/2026-10-22-frontend-excellence.md).

## Findings

- Reliability first: send awaits engine acceptance without a durable client outbox. Draft storage explicitly excludes
  attachments. Restoring a rejected draft in memory does not prove survival after a crash.
- Reading continuity remains partial: the runtime scrollTop map is not a durable message anchor. Pane widths and the
  last place already persist; extend them rather than adding another navigation subsystem.
- Panes, browser tabs, native visual pieces, keyboard separators, project memory, rewind, palette and quiet mode already
  exist. The blueprint often supplies regression criteria, not new features.
- Small correction candidate: ui/Composer.tsx:660 never clears the file input's selected value. Removing an attachment
  and choosing the same file again can produce no native change event. Reproduce with a focused regression, copy the
  FileList, then reset the input. Not fixed here.
- Generated frontend is another surface: the existing Faber UI skill/tokens govern its own screen. Generated projects
  need project-aware design/proof instructions and actual browser/accessibility verification without copying the desktop
  shell's style into every website. Reuse existing catalog/session-kit delivery (docs/product-map.md:498), never bundled
  third-party skill code or a second discovery path.

## R

| ID | Status | Decision | Evidence and limitation |
|---|---|---|---|
| R01 | partial | measure | send: No local pending message before engine response. |
| R02 | partial | measure | composer: Controls exist; 50 ms p95 unmeasured. |
| R03 | missing | defer | send: No durable outgoing-intent journal in send path. |
| R04 | unknown | measure | send: No send key found; engine deduplication not proven. |
| R05 | partial | keep | composer-check: Rejected attachments restore; restart not covered. |
| R06 | existing | keep | markdown: Incremental open-block rendering. |
| R07 | partial | measure | markdown: Stabilized blocks; incomplete-fence visual behavior unmeasured. |
| R08 | unknown | defer | map: No per-surface skeleton need established. |
| R09 | partial | measure | markdown: Incremental blocks; first-useful-line timing unmeasured. |
| R10 | unknown | measure | map: No cache freshness audit. |
| R11 | unknown | defer | map: No measured navigation bottleneck justifies prefetch. |
| R12 | existing | keep | search-check: Debounce and stale-result rejection covered. |
| R13 | partial | measure | quiet-check: Background coalescing exists; foreground frame cap unproven. |
| R14 | unknown | measure | map: No input-priority scheduling proof. |
| R15 | unknown | defer | looks: No purposeful shimmer gap established. |
| R16 | partial | measure | layout: Stable layout rules; shifts unmeasured. |
| R17 | partial | keep | markdown: Incremental rendering; source rewrites unaudited. |
| R18 | unknown | measure | map: Concurrency limits not audited end to end. |
| R19 | partial | measure | store: Atomic persistence; UI-thread latency unaudited. |
| R20 | unknown | measure | budget: Memory gate exists; per-interaction latency fallback unproven. |

## N

| ID | Status | Decision | Evidence and limitation |
|---|---|---|---|
| N01 | partial | keep | map: Single map exists; whole-product coherence not re-reviewed. |
| N02 | partial | keep | layout: Progressive disclosure documented; whole app unaudited. |
| N03 | partial | keep | palette: Existing palette; ordering interaction untested here. |
| N04 | missing | defer | parity: T10 needs second-window direction. |
| N05 | partial | keep | layout: Shared layout rules; cross-screen visual audit pending. |
| N06 | partial | keep | map: Context menus exist; every destructive item unaudited. |
| N07 | existing | keep | layout: Per-kind usual widths documented and implemented. |
| N08 | existing | keep | app: Per-kind widths persisted. |
| N09 | existing | keep | layout: Room-based docking and overlays. |
| N10 | unknown | defer | map: No focus-mode gap established. |
| N11 | partial | keep | keys-check: Area and conversation shortcuts covered. |
| N12 | partial | measure | app: Conversation panes retained; full navigation stack unproven. |
| N13 | partial | keep | site-check: Address Escape and reversible overlays; all previews unaudited. |
| N14 | partial | keep | search-check: Search opens exact old message; all target kinds unaudited. |
| N15 | partial | keep | site-check: Tab titles and identity covered; scroll restoration unaudited. |
| N16 | partial | measure | looks: Density preference exists; target-size audit pending. |

## C

| ID | Status | Decision | Evidence and limitation |
|---|---|---|---|
| C01 | partial | keep | composer-check: Growable composer and attachment cards; all sizes unaudited. |
| C02 | existing | keep | composer: Enter respects Shift and IME. |
| C03 | partial | defer | unsent: Only text persists; images and binary files do not. |
| C04 | partial | keep | unsent: Queued edit recovery exists; attachment lifetime unaudited. |
| C05 | partial | keep | map: Queue and steer documented; CLI-specific receipt audit pending. |
| C06 | partial | keep | map: Queue actions exist; full transition audit pending. |
| C07 | partial | defer | send: No durable local pending state. |
| C08 | partial | measure | thread: Status announcements exist; complete state audit pending. |
| C09 | partial | keep | parity: T7 records implemented branching/version navigation. |
| C10 | partial | keep | search-check: Message search and jump exist; Ctrl+F scope unaudited. |
| C11 | existing | keep | history-check: Up and search share list and retain draft. |
| C12 | partial | keep | composer: Mentions and commands exist; permissions unaudited here. |
| C13 | partial | keep | map: Ask-about-selection documented; citation consistency unaudited. |
| C14 | partial | measure | looks: Reading tokens exist; reading study absent. |
| C15 | partial | measure | app: Sticky-tail logic; anchor identity not durable. |
| C16 | partial | measure | map: Jump-to-latest exists; unread count semantics unaudited. |
| C17 | partial | keep | parity: Markdown export done; complete structured roundtrip unproven. |
| C18 | partial | defer | parity: Read aloud exists; T8 dictation absent. |
| C19 | partial | keep | history-check: Blur cancellation and focus cases covered. |
| C20 | missing | defer | parity: F6 temporary chat absent. |

## S

| ID | Status | Decision | Evidence and limitation |
|---|---|---|---|
| S01 | partial | measure | app: Place retained; all task restoration not audited. |
| S02 | partial | defer | app: Runtime scrollTop map; no persistent message anchor. |
| S03 | partial | measure | map: Expanded groups persist; search/filter kept only at runtime. |
| S04 | partial | keep | app: Per-conversation column and per-kind widths retained. |
| S05 | partial | keep | site-check: Tabs exist; restart and lazy recovery not run. |
| S06 | missing | defer | send: No outgoing-intent journal found. |
| S07 | partial | keep | rewind-check: Snapshot restore and partial recovery covered. |
| S08 | partial | keep | rewind-check: Undo and Redo covered; every user action unaudited. |
| S09 | partial | keep | store: Atomic replacement exists; outgoing journal absent. |
| S10 | partial | measure | send: Engine acceptance used; all optimistic domains unaudited. |
| S11 | partial | keep | parity: Terminal-session pickup done; area migration unaudited. |
| S12 | unknown | measure | map: Sleep/resume reconciliation not inspected. |
| S13 | missing | defer | parity: R2 phone control absent. |
| S14 | partial | keep | memory: Project memory controls exist; scope audit pending. |
| S15 | partial | keep | parity: C1 context details and compaction done. |

## A

| ID | Status | Decision | Evidence and limitation |
|---|---|---|---|
| A01 | partial | keep | thread: Live activity and check counts; all tool verbs unaudited. |
| A02 | partial | keep | map: Helper grouping exists; objective grouping unaudited. |
| A03 | partial | measure | map: States exist; all agent transitions unaudited. |
| A04 | partial | keep | thread: Counts sourced from events; complete progress audit pending. |
| A05 | partial | measure | map: Needs-you cards exist; newcomer study not run. |
| A06 | partial | keep | map: In-context permission cards; complete risk scope unaudited. |
| A07 | unknown | measure | map: Agent behavior cannot be proved from UI alone. |
| A08 | partial | keep | composer: Stop confirmation exists; pause semantics unaudited. |
| A09 | partial | keep | map: Helpers and scopes exist; all permission paths unaudited. |
| A10 | partial | keep | aside: Existing aside engine; full conversation parity not run. |
| A11 | partial | keep | history-check: Composer intervention exists; engine delivery audit pending. |
| A12 | existing | keep | research-check: Shared editable plan approval covered. |
| A13 | partial | measure | thread: Check counts exist; completion truth not fully audited. |
| A14 | partial | keep | map: Quota and cost exist; estimate provenance unaudited. |
| A15 | partial | keep | rewind-check: Recovery states covered for rewind; other tools unaudited. |
| A16 | partial | keep | map: Auto reason exists; explanation usefulness not studied. |
| A17 | partial | measure | map: Writer/reviewer evidence surfaces exist; truth audit pending. |
| A18 | unknown | defer | map: No absence digest implementation established. |
| A19 | unknown | defer | map: No dependency-view need established. |
| A20 | partial | keep | quiet-check: Game quiet behavior covered; all OSs unaudited. |

## B

| ID | Status | Decision | Evidence and limitation |
|---|---|---|---|
| B01 | partial | keep | site-check: Tab creation closing and keys covered; reopen unaudited. |
| B02 | existing | keep | site-check: URL/search and rejected schemes covered. |
| B03 | partial | measure | site-check: Real browser state; loading/redirect race audit pending. |
| B04 | partial | measure | map: Lifecycle policy exists; per-tab memory not measured here. |
| B05 | partial | keep | map: Human/agent ownership rules documented; races not run. |
| B06 | missing | defer | parity: K5 site allow/deny absent. |
| B07 | partial | measure | map: Session records exist; restart not run. |
| B08 | partial | keep | map: Page cards and sources exist; exact citation audit pending. |
| B09 | unknown | defer | map: No contextual screenshot handoff established. |
| B10 | missing | defer | parity: M6 other-app control absent. |
| B11 | missing | defer | parity: M6 other-app control absent. |
| B12 | partial | keep | history-check: Drop and attach covered; all native surfaces unaudited. |
| B13 | partial | measure | composer: Images pasted; rich tables/files fallbacks unaudited. |
| B14 | partial | keep | quiet-check: Quiet windows tested; focus stealing whole app unaudited. |
| B15 | partial | keep | map: Save/show/open actions exist; native behavior not run. |
| B16 | missing | defer | parity: P6 SSH absent. |

## V

| ID | Status | Decision | Evidence and limitation |
|---|---|---|---|
| V01 | partial | keep | pieces-check: Ten native kinds exist; choice quality is agent behavior. |
| V02 | partial | keep | markdown: Sanitized Markdown exists; all formats unaudited. |
| V03 | existing | keep | pieces-check: Bounded arithmetic and ten native kinds covered. |
| V04 | existing | keep | pieces-check: Saved choices and remount recovery covered. |
| V05 | partial | keep | visual: HTML/SVG/Mermaid use existing pane. |
| V06 | partial | keep | visual: Bounded saved file versions documented. |
| V07 | unknown | defer | visual: No synchronized output comparison established. |
| V08 | unknown | defer | pieces-check: No large interactive-table audit. |
| V09 | partial | measure | pieces-check: Semantic native data exists; chart-scale audit pending. |
| V10 | partial | keep | pieces-check: Unfinished fences remain code; all visual runtimes unaudited. |
| V11 | partial | keep | map: Native save cards; origin/version audit pending. |
| V12 | partial | defer | parity: M5 selected-artifact comments still a gap. |
| V13 | partial | keep | visual: Isolated runtime documented; threat tests not run. |
| V14 | partial | measure | visual: Bounded file versions; all large previews unaudited. |
| V15 | partial | measure | research-check: Report links exist; source truth not audited. |
| V16 | unknown | defer | parity: Image/voice/video capabilities need product work. |

## Evidence key

Paths below identify inspected source/coverage; line anchors can move after edits.

- send: [ui/App.tsx:992](../../ui/App.tsx#L992).
- composer: [ui/Composer.tsx:443](../../ui/Composer.tsx#L443).
- composer-check: [tests/ui/composer.check.ts:50](../../tests/ui/composer.check.ts#L50).
- markdown: [ui/markdown.tsx:135](../../ui/markdown.tsx#L135).
- map: [docs/product-map.md:403](../product-map.md#L403).
- search-check: [tests/ui/search-messages.check.ts:27](../../tests/ui/search-messages.check.ts#L27).
- quiet-check: [tests/ui/quiet.check.ts:16](../../tests/ui/quiet.check.ts#L16).
- looks: [docs/design/looks.md:5](../design/looks.md#L5).
- layout: [docs/design/layout.md:21](../design/layout.md#L21).
- store: [engine/src/store.rs:91](../../engine/src/store.rs#L91).
- budget: [testkit/src/screen/app.rs:12](../../testkit/src/screen/app.rs#L12).
- palette: [ui/Palette.tsx:1](../../ui/Palette.tsx#L1).
- parity: [docs/product/parity.md:25](../product/parity.md#L25).
- app: [ui/App.tsx:317](../../ui/App.tsx#L317).
- keys-check: [tests/ui/shortcuts.check.ts:15](../../tests/ui/shortcuts.check.ts#L15).
- site-check: [tests/ui/site-tabs.check.ts:24](../../tests/ui/site-tabs.check.ts#L24).
- unsent: [ui/unsent.ts:80](../../ui/unsent.ts#L80).
- thread: [ui/Thread.tsx:511](../../ui/Thread.tsx#L511).
- history-check: [tests/ui/prompt-history.check.ts:60](../../tests/ui/prompt-history.check.ts#L60).
- rewind-check: [tests/ui/rewind.check.ts:60](../../tests/ui/rewind.check.ts#L60).
- memory: [ui/ProjectMemory.tsx:1](../../ui/ProjectMemory.tsx#L1).
- aside: [engine/src/engine/aside.rs:1](../../engine/src/engine/aside.rs#L1).
- research-check: [tests/ui/research.check.ts:63](../../tests/ui/research.check.ts#L63).
- pieces-check: [tests/ui/visual-pieces.check.ts:53](../../tests/ui/visual-pieces.check.ts#L53).
- visual: [docs/design/visual-answers.md:6](../design/visual-answers.md#L6).

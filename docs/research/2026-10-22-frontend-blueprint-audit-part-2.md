# Frontend blueprint evidence audit: part 2

[Method, findings and R01–V16](2026-10-22-frontend-blueprint-audit.md). Static inspection only; cited checks were not run.
Status/decision definitions and snapshot are those of part 1. This file covers T01–F16.

## T

| ID | Status | Decision | Evidence and limitation |
|---|---|---|---|
| T01 | partial | measure | send: Engine reply separates acceptance; delivery journal missing. |
| T02 | partial | keep | map: Permission modes exist; full operation taxonomy unaudited. |
| T03 | partial | measure | map: Just-in-time cards exist; necessity not re-reviewed. |
| T04 | partial | keep | visual: Preview isolation; systemwide least privilege unaudited. |
| T05 | partial | keep | rewind-check: Conflict-aware rewind exists; every destructive action unaudited. |
| T06 | partial | keep | rewind-check: Restore preview and conflicts; external publishing unaudited. |
| T07 | partial | measure | rewind-check: Partial restore never claims completion; other domains unaudited. |
| T08 | partial | keep | rewind-check: Translated recovery paths; all errors unaudited. |
| T09 | partial | measure | map: Cross-vendor review exists; independence truth unaudited. |
| T10 | partial | keep | map: Catalog and model provenance; complete cost audit pending. |
| T11 | partial | keep | map: Privacy rules exist; full enforcement not audited. |
| T12 | partial | keep | visual: Untrusted preview isolation; all tool text unaudited. |
| T13 | partial | measure | map: Work logs and receipts exist; secret filtering unaudited. |
| T14 | partial | keep | map: Update controls exist; rollback not run. |

## P

| ID | Status | Decision | Evidence and limitation |
|---|---|---|---|
| P01 | partial | measure | app: Latest-message window grows upward; bounded return pruning unproven. |
| P02 | partial | keep | thread: Folded activity exists; all DOM hydration unaudited. |
| P03 | partial | measure | map: Pane lifecycle rules; release measurement not run. |
| P04 | partial | keep | quiet-check: Hidden updates coalesce; every timer unaudited. |
| P05 | existing | keep | quiet-check: Latest events batch on return. |
| P06 | partial | measure | search: Search worker exists; Markdown stays synchronous. |
| P07 | partial | keep | budget: 400 MB check exists; current run not executed. |
| P08 | unknown | measure | budget: No fresh p95 frame measurements. |
| P09 | existing | keep | styles: Content visibility applied to replies and blocks. |
| P10 | partial | keep | search: Background indexing exists; cancellation implementation unaudited. |
| P11 | partial | measure | thumb: Thumbnail component exists; decoded media cost unaudited. |
| P12 | partial | keep | map: Active work and drafts protect updates; not run. |
| P13 | partial | keep | quiet-check: Game quiet checks exist; GPU cost not measured. |
| P14 | partial | measure | budget: Memory gate exists; visual CPU/frame regression budgets incomplete. |

## X

| ID | Status | Decision | Evidence and limitation |
|---|---|---|---|
| X01 | partial | measure | focus: Semantic and keyboard foundations; AA conformance unverified. |
| X02 | partial | measure | layout: Keyboard separators; all features unverified. |
| X03 | partial | measure | styles: Focus rules exist; occlusion audit absent. |
| X04 | partial | keep | keys-check: Shortcut sheet restores focus; all dialogs unaudited. |
| X05 | partial | keep | thread: Polite atomic activity announcements; real reader not tested. |
| X06 | partial | measure | composer: Named native controls; full surface audit absent. |
| X07 | partial | keep | appearance: System motion watcher; all components unaudited. |
| X08 | partial | keep | layout: Keyboard alternatives to separators; other drag actions unaudited. |
| X09 | unknown | measure | styles: Target size/spacing not measured. |
| X10 | partial | measure | styles: Forced-colors CSS exists; OS mode not run. |
| X11 | partial | measure | rewind-check: 720 px German restore check; full zoom matrix absent. |
| X12 | partial | keep | map: Manual locales exist; native-speaker review incomplete. |
| X13 | partial | measure | thread: Words supplement status; every visual signal unaudited. |
| X14 | unknown | measure | map: No fresh Narrator/VoiceOver/Linux reader results. |

## D

| ID | Status | Decision | Evidence and limitation |
|---|---|---|---|
| D01 | partial | keep | looks: Shared tokens; complete duplicate-token audit absent. |
| D02 | partial | keep | looks: Meaning colors fixed; every use unaudited. |
| D03 | partial | measure | looks: Composition rules; screenshot critique not run. |
| D04 | partial | keep | looks: Motion purpose required; full usage audit absent. |
| D05 | partial | keep | looks: Frequency tiers; all animation sites unaudited. |
| D06 | partial | defer | map: Existing list helpers; need for new FLIP unmeasured. |
| D07 | partial | measure | looks: Disclosure rules; focus/position matrix not run. |
| D08 | unknown | defer | parity: No supported haptic need established. |
| D09 | partial | measure | map: Localized action text; copy audit incomplete. |
| D10 | partial | keep | looks: Idle art restricted; visual critique pending. |
| D11 | partial | measure | looks: Type tokens; rendering across OSs unverified. |
| D12 | partial | measure | layout: Native affordances documented; real OS study absent. |
| D13 | partial | measure | sentinel: Screenshot/first-user process exists; not executed here. |
| D14 | partial | measure | looks: Shared structure; all themes not inspected. |

## L

| ID | Status | Decision | Evidence and limitation |
|---|---|---|---|
| L01 | partial | measure | looks: Density settings; every target and line unaudited. |
| L02 | partial | keep | looks: Safe theme values; arbitrary-value audit pending. |
| L03 | unknown | defer | map: No learned-quick-action need established. |
| L04 | partial | keep | app: Model choice retained; routing explanation unaudited. |
| L05 | partial | keep | memory: Project memory provenance; learned preference flow unaudited. |
| L06 | partial | keep | memory: Project-scoped files; isolation not exercised. |
| L07 | partial | measure | memory: Decision author/keep model; promotion paths unaudited. |
| L08 | partial | measure | map: Five locales; full long-text/RTL scope not tested. |
| L09 | partial | keep | i18n: Locale formatting helpers; full surface coverage unaudited. |
| L10 | partial | measure | keys-check: Central shortcut list; macOS display not tested. |
| L11 | partial | keep | map: Defaults and Settings placement; setup study pending. |
| L12 | partial | measure | map: Scopes documented; all preference readers unaudited. |

## Q

| ID | Status | Decision | Evidence and limitation |
|---|---|---|---|
| Q01 | partial | keep | tray: Attention hierarchy exists; all completion cases unaudited. |
| Q02 | partial | keep | tray: Real running/needs-you counts; not run. |
| Q03 | partial | keep | tray: Deduplication code exists; race audit pending. |
| Q04 | partial | keep | quiet-check: Games quiet; DND OS matrix not exercised. |
| Q05 | unknown | defer | map: No absence digest established. |
| Q06 | missing | defer | parity: R1 scheduling absent. |
| Q07 | partial | defer | map: Standing goals exist; generic alerts unaudited. |
| Q08 | partial | measure | map: Offer-bigger flow exists; appropriateness not studied. |
| Q09 | unknown | measure | map: No interruption count measured. |
| Q10 | partial | measure | thread: Waiting/status words; all idle reasons unaudited. |

## O

| ID | Status | Decision | Evidence and limitation |
|---|---|---|---|
| O01 | partial | keep | map: First-run/tutorial exist; beginner study not rerun. |
| O02 | partial | keep | map: Actionable empty states documented; all states not seen. |
| O03 | partial | measure | map: Tutorial exists; contextual education effectiveness unknown. |
| O04 | partial | keep | map: Installed guide exists; version-grounding audit pending. |
| O05 | partial | keep | map: Account repair paths; offline matrix not run. |
| O06 | partial | keep | composer: Slash/mentions and palette available. |
| O07 | partial | measure | map: Tutorial skip/next exist; every step not exercised. |
| O08 | partial | measure | map: Permission words exist; comprehension not studied. |
| O09 | partial | keep | map: Whats-new exists; absence digest distinction unaudited. |
| O10 | partial | keep | map: Suggestions fill composer; no automated-send audit here. |

## M

| ID | Status | Decision | Evidence and limitation |
|---|---|---|---|
| M01 | unknown | measure | sentinel: No fresh beginner/expert success study. |
| M02 | unknown | measure | budget: No fresh meaningful-paint timing. |
| M03 | unknown | measure | budget: No fresh input p95 measurement. |
| M04 | unknown | measure | budget: No request-to-proof timing. |
| M05 | partial | measure | composer-check: Recovery checks exist; rates not measured. |
| M06 | unknown | defer | parity: No blind paired competitor study. |
| M07 | partial | measure | keys-check: Keyboard checks exist; AA gate completeness unknown. |
| M08 | partial | keep | sentinel: Visual checks exist; fresh captures not made. |
| M09 | partial | measure | sentinel: First-user runner exists; not run. |
| M10 | partial | measure | budget: Memory runner exists; no fresh comparison. |
| M11 | unknown | measure | parity: Cross-OS runtime matrix absent. |
| M12 | partial | measure | map: Quota/setup recovery exists; full degraded matrix absent. |
| M13 | partial | defer | store: Atomic-save tests; outgoing crash cases missing. |
| M14 | partial | keep | map: Product-map/coherence process exists; current changes not reviewed. |
| M15 | partial | defer | map: Privacy default exists; local metric dashboard unaudited. |
| M16 | partial | measure | map: Light benchmark exists; full benchmark not built. |

## F

| ID | Status | Decision | Evidence and limitation |
|---|---|---|---|
| F01 | partial | defer | map: Auto routes model; universal-intent dispatch not established. |
| F02 | partial | measure | map: File cards/proofs exist; complete provenance audit pending. |
| F03 | partial | measure | map: Cross-vendor helper/review exists; independence audit pending. |
| F04 | partial | keep | map: Steer/control tools exist; every intervention receipt unaudited. |
| F05 | partial | defer | unsent: Draft attachments absent and scroll runtime-only. |
| F06 | partial | measure | map: Auto rationale/quota; causal benefit evidence unaudited. |
| F07 | partial | keep | map: Proof receipts exist; reproducible-one-click flow unaudited. |
| F08 | unknown | defer | map: No last-observation change digest established. |
| F09 | partial | keep | layout: Per-content pane sizes and memory exist. |
| F10 | partial | measure | budget: Memory budget exists; premium feel requires measured review. |
| F11 | partial | keep | rewind-check: Safe restore preview exists; all external operations unaudited. |
| F12 | partial | keep | sentinel: Synthetic UX sentinel exists; autonomous remediation unaudited. |
| F13 | unknown | defer | parity: No blind provider-comparison implementation established. |
| F14 | missing | defer | parity: M6 other-app selection/control absent. |
| F15 | partial | measure | map: Technical details separate facts; explanation usefulness untested. |
| F16 | unknown | defer | map: No teach-once tooltip persistence established. |

## Evidence key

Paths below identify inspected source/coverage; line anchors can move after edits.

- send: [ui/App.tsx:992](../../ui/App.tsx#L992).
- map: [docs/product-map.md:403](../product-map.md#L403).
- visual: [docs/design/visual-answers.md:6](../design/visual-answers.md#L6).
- rewind-check: [tests/ui/rewind.check.ts:60](../../tests/ui/rewind.check.ts#L60).
- app: [ui/App.tsx:317](../../ui/App.tsx#L317).
- thread: [ui/Thread.tsx:511](../../ui/Thread.tsx#L511).
- quiet-check: [tests/ui/quiet.check.ts:16](../../tests/ui/quiet.check.ts#L16).
- search: [engine/src/search.rs:253](../../engine/src/search.rs#L253).
- budget: [testkit/src/screen/app.rs:12](../../testkit/src/screen/app.rs#L12).
- styles: [ui/styles.css:517](../../ui/styles.css#L517).
- thumb: [ui/Thumbs.tsx:1](../../ui/Thumbs.tsx#L1).
- focus: [ui/focus.ts:1](../../ui/focus.ts#L1).
- layout: [docs/design/layout.md:21](../design/layout.md#L21).
- keys-check: [tests/ui/shortcuts.check.ts:15](../../tests/ui/shortcuts.check.ts#L15).
- composer: [ui/Composer.tsx:443](../../ui/Composer.tsx#L443).
- appearance: [ui/appearance.ts:70](../../ui/appearance.ts#L70).
- looks: [docs/design/looks.md:5](../design/looks.md#L5).
- parity: [docs/product/parity.md:25](../product/parity.md#L25).
- sentinel: [docs/dev/sentinel.md:50](../dev/sentinel.md#L50).
- memory: [ui/ProjectMemory.tsx:1](../../ui/ProjectMemory.tsx#L1).
- i18n: [ui/i18n.ts:1](../../ui/i18n.ts#L1).
- tray: [src-tauri/src/tray/decide.rs:128](../../src-tauri/src/tray/decide.rs#L128).
- composer-check: [tests/ui/composer.check.ts:50](../../tests/ui/composer.check.ts#L50).
- store: [engine/src/store.rs:91](../../engine/src/store.rs#L91).
- unsent: [ui/unsent.ts:80](../../ui/unsent.ts#L80).

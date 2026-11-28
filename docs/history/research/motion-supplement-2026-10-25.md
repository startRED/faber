# Historical motion evidence supplement

Status: historical preservation only. This page does not prove the current combined tree, integration, present
visual behavior, resource cost or completion of task 997. No application or provider was run to prepare it.

Source: [PR #319](https://github.com/startRED/faber/pull/319), head `03daeeda82d0e12bcaad1b8fdfd4dd781fa3cf8b`.
Missing-evidence comparison base: `44d746ea247184561b54dd9f7ed0b5980a24c2c2` (2026-10-25).
The excerpts below preserve the source wording verbatim, including its historical findings, scores and limitations.
Claims such as "now", "remains open" and "integration remains unfinished" describe that source, not current status.
Current living documentation takes precedence. The original three research pages are retained unchanged.

## Excerpt sources

| Original page | Preserved source lines |
| :--- | :--- |
| [motion-chrome-timing-2026-10-24.md](motion-chrome-timing-2026-10-24.md) | 34–39: later window-frame evidence and its capture-time limitation |
| [motion-frequent-input-2026-10-24.md](motion-frequent-input-2026-10-24.md) | 30–32: evidence-root context; 49–75: focused attachment capture |
| [motion-inventory-2026-10-24.md](motion-inventory-2026-10-24.md) | 11–14, 17, 23, 28–29, 48–51 and 60–186: inventory updates and focused observations |

## Later window-frame evidence (historical excerpt)

Visual critique: before 39/40, after 39/40, by independent read-only `/root/review_motion_frequent`. At 720x480 the
eight rubric scores were 5,5,5,5,5,5,5,4; at 1280x800 they were all 5. One fit point was lost in both small captures:
the "Melhor" heading overlaps the Faber title-bar mark. It is present before and after, so this is not a timing-change
regression. The later [window-frame correction](notes-window-frame-2026-10-24.md) records reproduction of the viewport
starting behind the titlebar, its correction and independent 40/40 after critique. Integration remains unfinished;
that note also records the missing capture-time executable digest. No question-error image was captured or scored.

## Attachment evidence-root context (historical excerpt)

Evidence is retained outside disposable caches at
`E:/Documentos/ProjetosIA/faber-evidence/motion-frequent-input-20261024/`: sixteen PNGs, two screenshot indexes,
negative-control and corrected composition logs, SHA-256 digests and the corrected source/executable identities.

## Focused slow capture: attachments and the message bar

The first successful focused capture is retained in the evidence root above under
`attachments-720-full-fixture04/run-04/`: four PNGs, before/after provenance and a detailed receipt. Three earlier
attempts failed and remain retained: missing Git before launch, an incomplete clock sample, then a timeout awaiting
attachment disclosure and a completed response. Independent diagnosis identified an incompatible replay input;
that failed capture did not establish whether submission occurred. None produced successful screenshots or counts
as visual approval.

The successful run used the existing pinned app, driver and fake CLI, without compilation. A detached runtime
worktree at `af853a33` changed only the replay's first input to seven exact 1x1 PNG blocks and the original question,
following the existing attachment-check pattern. This is synthetic content, not a live provider response.
Before/after source and executable identities matched; the runtime fixture was intentionally modified and recorded.

At 720x480, full motion and 0.25x playback, the four images show hidden message actions, revealed hover actions,
attachment expansion in progress and all seven attachments expanded. Submission waited for Send to become enabled,
clicked the composer and pressed Enter; the field cleared. The normal resource gate was acquired. The clock passed
on sample two of the bounded three-sample policy (184 frames, median gap about 5.3 ms); 235 expansion observations
include intermediate opacity and all three newly revealed attachments reaching opacity 1. Exit was natural with
code 0 and the CLEAN marker. Parent tool receipts: launch `476e8d`, identity/self-check/dry-run `84d26c`.

Fresh read-only reviewer `/root/fixture04_review` (`gpt-6.1-sol`) inspected all four images and receipts. It found no
evident visual defect in this scope: the hover bar did not move content and the expanded thumbnails stayed aligned
without visible overlap. Its preparation and result reports are retained verbatim beside the capture folder.
This is a focused observation, not a replacement for the scored composition critique or the full task-997 walk.
The 1x1 images do not cover real-photo quality, decoding load or varied aspect ratios. Slowed playback does not
measure normal-speed smoothness, memory or processor cost. Integration remains unverified.

## Inventory updates (historical excerpts)

| Surface | Existing implementation and checks | Remaining visual observation |
| :--- | :--- | :--- |
| Settings and other panels | `Settings.tsx`, `motion.ts` animateList; `motion.check.ts`, sentinel panels | General/Privacy cross-fade observed at 720x480/full/0.25x, with an incorrect language label in an exit copy still open; panel open/close, interruption and reduced/none remain. |
| Context and permission menus | `.cmenu`, `.st-menu`, keepForExit; `contextmenu.check.ts`, `settings.check.ts` | One message menu and one Settings permission list observed at 720x480/full/0.25x, including Escape focus return. Same-message rapid reversal is now observed; edge placement, option activation and remaining motion modes still need observation. |
| Permission/question cards, toast and errors | `Ask.tsx`, `.ask`, `.toast`, `.quota-card`; `question.check.ts`, sentinel permission | One normal lifecycle is observed below; visual error/retry paths and the complete visible exit remain unobserved. |
| Message actions and thumbnails | Shared Thread and thumbnail CSS; `viewer.check.ts` | Four focused 720x480/full/0.25x states independently inspected; real photos, varied ratios, interruption and remaining motion modes still need observation. |

These check names identify existing coverage, not a claim that this investigation executed all of them. The focused
attachment and message-bar capture is recorded in `motion-frequent-input-2026-10-24.md`; it does not approve this inventory.

A pre-existing small-window overlap between a notes heading and the title-bar mark lost one fit point. The later
[window-frame correction](notes-window-frame-2026-10-24.md) records its reproduction, fix and independent 40/40 after
critique. The question-error state was not photographed. Integration and the complete slow walk remain unfinished. Details are in `motion-chrome-timing-2026-10-24.md`; evidence is retained outside caches at
`E:/Documentos/ProjetosIA/faber-evidence/motion-chrome-timing-20261024/`.

This is not a complete inventory walkthrough. Remaining menu cases, Catalog details, native picker behavior and recoverable-error
animation still need explicit observation and independent scores. Thumbnail expansion and message actions now have
one independently inspected four-state slow capture; their broader cases and individual scores remain unfinished.

## Focused message context-menu observation

Evidence is retained at `E:/Documentos/ProjetosIA/faber-evidence/context-menu-motion-20261024/run-01/`: four PNGs,
receipts and before/after provenance. One hidden native run opened the menu by right-clicking a real synthetic
message, then pressed Escape. It used the original text-only replay, clean runtime source `af853a33`, clean runner
source `5b702765` and the pinned existing app, driver, fake CLI and probe. It made no product change or live provider
request. The reviewed capture hash was checked before launch. Parent launch receipt: `2c2f69`.

At 720x480, pt-BR, Oficina, full motion and 0.25x CSS playback, the clock passed on sample two (186 frames, median
gap about 5.2 ms). The bounded collector retained 224 menu observations, including 121 intermediate entrance frames
and 15 leaving frames. The open menu had three labelled actions, stayed inside the window and held focus itself,
as intended for pointer opening. Escape made the leaving menu inert and pointer-disabled, then removed it and
returned visible focus to the exact same message node. The exit PNG still contains the leaving menu; observations
after that image are recorded separately. Source and executable identities matched before/after; the normal resource
gate, natural exit 0 and CLEAN marker were retained.

Fresh read-only reviewer `/root/observation_evidence_review` (`gpt-6.1-sol`) inspected all four images, frame records
and provenance, with no blocker in this scope. The open and closed-focus endpoints each scored 40/40 on the existing
eight-line composition rubric, with no deductions; transitional images were inspected without final-state scores.
These are two states of one unchanged app, not a before/after correction comparison. Cards checked: C006, C007, C009,
C011. The preparation and result reports are retained verbatim beside the capture folder.

The JavaScript removal timer is unchanged by slowed CSS playback: this capture observed removal about 80.7 ms after
the leaving state, with the last sampled opacity about 0.907. It does not prove a complete slowed fade to opacity 0
or diagnose a normal-speed exit defect. Images and DOM observations are sequential, not atomic or exact animation
percentages. That message-menu run did not cover permission menus, edge placement, rapid reversal, reduced/none, normal-speed performance, runtime
memory/processor cost, integration and full task-997 approval remain unverified by this focused run. Item navigation,
action activation and destructive-action confirmation were not exercised; external desktop focus was not measured.

## Focused Settings permission-list observation

Evidence is retained at `E:/Documentos/ProjetosIA/faber-evidence/settings-permission-motion-20261024/`.
One hidden native run opened the existing permission selector and closed it with Escape at 720x480, pt-BR,
Oficina, full motion and 0.25x CSS playback. It selected no permission and made no provider request. The isolated
sandbox alone received the appearance prerequisite; the personal app and its settings were not changed.
Clean runner source `a7d93dfe`, clean runtime source `af853a33` and the original cache-3 app, driver, fake CLI,
fixture and probe were pinned. The reviewed capture hash was checked before the normal resource gate and shared
lease. Parent launch receipt: `ac1d6d`; natural exit 0 and CLEAN were recorded, with identical provenance before/after.

The four original images show entrance, settled open, exit and closed focus. All five options and their descriptions
fit inside the window and Settings scroll pane. The list measured x340, y81.828125, width320, height303.796875;
the deliberate pane scroll is not a clipped menu. Exactly one option stayed selected: `set-permissions-pick-opt-4`.
The button label remained `Acesso total`, and the persisted permission remained `dontAsk` before and after. Escape
kept Settings open, removed the list and returned visible focus to the exact same connected combobox.

The clock passed on sample one with 179 frames. The passive collector retained 241 observations, including 122
intermediate entrance frames and 15 leaving frames plus one leaving event. All 16 leaving samples kept the same
selected option, remained connected and were inert and pointer-disabled. Removal was observed about 84.8 ms after
the leaving event. The exit image still shows the list; the later closed-focus image has no visible ghost.

Fresh read-only reviewer `/root/observation_evidence_review` inspected all four images, records and provenance.
The open and closed-focus endpoints each scored 40/40 on the existing eight-line composition rubric, with no
deductions; the two transitional images were inspected without final-state scores. This is one unchanged app,
not a before/after correction. The verbatim preparation/result reviews and image manifest are beside the capture.
Result review SHA-256: `e5ba8b1bbb262329fb1c55485b6680d683b099129b87d6b7acb0898ba0d38d4a`.

Slowed CSS does not stretch the JavaScript removal timer. Sequential images and records are not atomic and do not
prove an exact animation percentage, complete slowed fade, normal-speed smoothness or resource cost. This run does
not evaluate permission policy, activate an option, cover other sizes/themes/languages/reduced/none, establish
screen-reader behavior, approve integration or complete task 997. No product correction follows from this observation.

## Focused Settings section transition: visual finding remains open

Evidence is retained at `E:/Documentos/ProjetosIA/faber-evidence/settings-section-motion-20261024/`.
One hidden native run navigated General to Privacy and back, at 720x480, pt-BR, Oficina, full motion and 0.25x
CSS playback, without activating an internal control or provider. It used the same clean runner/runtime sources
and original cache-3 artifacts as the permission-list observation. The reviewed capture hash was checked before
the normal shared lease/resource gate. Parent launch receipt: `5c7e88`; natural exit 0, CLEAN and identical captured
provenance before/after were recorded. The permission value stayed `dontAsk`; this is not a language-save check.

Fresh read-only reviewer `/root/observation_evidence_review` inspected all five original PNGs and the frame records.
The initial General and both settled destination states each scored 40/40 on the existing eight-line composition
rubric, with no visible ghost at rest. Both destination buttons retained exact native focus, selection and heading;
their original references stayed connected, scrollTop was zero, and exit hosts and live animations were gone.
The deliberate nonzero-scroll reset and interruption cases were not exercised.

The transient Privacy-entry image exposes a concrete visual defect: the departing General copy shows
`Igual ao sistema` for Language, while initial and returned General show `Português (Brasil)`. The automatic
assertions passed but did not retain that selector's value or selectedIndex. The evidence establishes an unfaithful
visual copy, not a persisted-language change or its cause. This finding remains open; the endpoint scores do not
approve the whole transition. The two transient images were inspected without final-state scores.

The two collectors retained 183/221 frames, with 125/144 intermediate entrance frames and 102/103 frames containing
an old-section clone. Every recorded clone was inert, aria-hidden and pointer-disabled. Hosts disappeared about
642.9/643.7 ms after each collector started; live animations reached rest about 1070.9/1193.4 ms. These sampled
times include the native click and CSS slowdown, and are not normal interaction latency. Recorded opacity changes
confirm a cross-fade. All sampled computed transforms had zero displacement; variable-based keyframes alone do
not establish vertical movement. No memory, processor or normal-speed performance conclusion follows.

The verbatim result review and five-image manifest are beside the capture. Result review SHA-256:
`09ed9f1000c9c8bdaa6f5c9929467328ea2cd14b5b9926058cbabc86baded645`.
Other sections, sizes, themes, languages, reduced/none, rapid reversal, resource cost, integration and complete
task-997 approval remain unverified by this focused run. No correction has been verified for the visual finding.

## Focused same-message context-menu reversal

Evidence is retained at `E:/Documentos/ProjetosIA/faber-evidence/context-menu-reversal-20261024/run-01/`. One hidden native run used the existing text-only fixture at 720x480, pt-BR, Oficina, full motion and 0.25x CSS playback. After opening the real message menu with the native right-button operation, one page evaluation dispatched Escape on the open menu and then a right-button `contextmenu` event on the same still-connected message node. The measured pair took 0.9 ms against the actual 80 ms JavaScript exit timer. At the immediate snapshot, the old menu remained connected, marked leaving, inert and pointer-disabled; reopening produced a fresh keyed panel, exactly one active menu, and the same message index. The labels were `Copiar`, `Editar` and `Voltar pra antes desta mensagem`; the reopened menu was inside the viewport and focused. A settled check retained the same labels and focus. A final native Escape removed all menu nodes and returned focus to the exact connected trigger; `closed-focus.png` shows no ghost.

The normal shared resource slot was held through child exit. The run exited naturally with code 0 and `CLEAN`; before/after source and artifact identities match. Five PNGs, receipts and provenance are in the evidence folder. Launcher exit receipt: `3d141e`. The independent actual-result review at `independent-result-review.txt` (SHA-256 `9c52f058ad7c2ead3fd2187c19754e2e7c7a61beb5ca0a759c0aff124069ed0b`) passed only this captured interaction. It scored the settled open, reversal and closed-focus images 40/40 each on the local eight-line rubric; this is not a before/after comparison or broad visual approval.

The event pair is synthetic in one JavaScript task, so it does not measure physical input timing. CSS playback at 0.25x does not slow the JavaScript removal timer. The immediate reversal snapshot does not retain the old node's action labels, so old-label continuity during that exact interval is not directly proven. Only 14 final-exit frames sampled the leaving state, down to opacity 0.912486; a complete slowed fade was not observed. Edge placement, action activation, reduced/none, normal-speed performance, resource cost, integration and full task-997 approval remain unverified. The earlier rejected preparation and stale-pin red receipt remain preparation history, not a product failure.

## Focused normal question-card lifecycle

Evidence is retained at `E:/Documentos/ProjetosIA/faber-evidence/question-card-motion-20261024/run-01/`.
One hidden native run used the existing full-access fake recording through an isolated fixture-only checkout,
at 720x480, pt-BR, Oficina, full motion and 0.25x CSS playback. It selected SQLite by the visible option and sent
through the real card. The clock passed on its first sample (188 frames, median gap about 5.2 ms); this is a capture
clock result, not application FPS. The normal lease/resource guard, natural exit 0, CLEAN marker and identical
before/after source/executable snapshots were recorded. No live provider or evaluator was called.

The independent reviewer inspected all five images and the receipts. The selected and completed states scored
40/40; the open state scored 39/40 because the assistant identity/status line is partly clipped at the top of the
conversation viewport. Its baseline/regression status is unknown; the question, choices and footer fit. Completion
has no card, running Stop control or error/stopped note, and an empty composer. The recording finishes successfully
without assistant text; it does not demonstrate generated answer content or transmission of SQLite in the reply.

The exit image contains no visible card: its opacity was already zero although the node remained mounted and
leaving. Recorded height decreased from 179.25 to 175.328125 before removal, demonstrating only the beginning of
collapse. Forty-seven observer records include duplicates, 46 connected records and one removal; they are not 47
transitions. The JavaScript removal timer is not slowed by CSS playback. A complete visible exit fade/fold, normal-speed
feel, resource cost, other sizes/languages/motion modes, integration and task-997 completion remain unverified.
Result review SHA-256: `a3f74d5af1e39b65abec6fc83d1d21d5386131ce65575f075f1fc1402de7354a`.

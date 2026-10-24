# Motion inventory for task 997

Status: unfinished. This inventory prepares the required 0.25x walkthrough; source inspection is not runtime or visual
approval. The source-only inventory was collected independently by `/root/catalog_research`. Local executable and
visual evidence is named separately below. No surface has a complete task-997 approval.

Rules: `docs/design/looks.md`, `.claude/skills/faber-ui/SKILL.md` and `docs/dev/sentinel.md`.

## Surfaces and remaining observations

| Surface | Existing implementation and checks | Remaining visual observation |
| :--- | :--- | :--- |
| Settings and other panels | `Settings.tsx`, `motion.ts` animateList; `motion.check.ts`, sentinel panels | Open/close, section cross-fade, interruption and reduced/none. |
| Context and permission menus | `.cmenu`, `.st-menu`, keepForExit; `contextmenu.check.ts`, `settings.check.ts` | One message-menu entrance, exit and Escape focus return observed at 720x480/full/0.25x; permission menus, edges, rapid reversal and remaining motion modes still need observation. |
| Native selects | Customizable picker with native fallback in `styles.css`; Settings checks | Supported-picker entrance/exit and fallback behavior. |
| Palette and Find | `.pal`, `.find`; palette interaction and `find.check.ts` checks | Both separate entrance and exit paths. |
| Permission/question cards, toast and errors | `Ask.tsx`, `.ask`, `.toast`, `.quota-card`; `question.check.ts`, sentinel permission | Retrying actual error states, including the later error shorthand found below. |
| Composer and attachments | `Composer.tsx`, reveal, attachment list, Ghost; composer/idle/review-motion checks, sentinel composer | Wrapping, send, interrupted growth and thumbnail expansion. |
| Running column and reading stage | `App.tsx`, motion.ts; interactions/shell checks, sentinel panels | Docked/overlay width, open/close, cancellation. Exit-duration work belongs to another conversation. |
| Sidebar and project folds | `App.tsx`, `.fold`, shared resize; shell/motion/review-motion checks | Focus mode, narrow overlay, scrim, folding and resize cancellation. |
| Catalog | `Catalog.tsx`, selection fade, empty pulse; `catalog.check.ts` | Browse, details, filter and pagination under full/reduced/none. |
| First run, setup and tutorial | `FirstRun.tsx`, `Setup.tsx`, coach/ring; setup/styles/tutorial/motion checks, sentinel tutorial | Step/back/skip/retry, ring travel and final exit. |
| Message actions and thumbnails | Shared Thread and thumbnail CSS; `viewer.check.ts` | Four focused 720x480/full/0.25x states independently inspected; real photos, varied ratios, interruption and remaining motion modes still need observation. |
| Viewer, quote popup and reply states | Viewer CSS, swap icons, animateDetails; viewer/motion/review-motion checks | Opening origin, image swap, cleanup and interrupted disclosures. |
| Home effects and theme changes | Welcome, Ghost, Ambient, Hole, native View Transition; idle/hole/quiet checks | Distinguish expressive home scenes from chrome; focus loss, hidden stop, theme swap and reduced/none. |
| What's new | `WhatsNew.tsx`, `.notes-group`; `notes.check.ts` and new all-theme motion check | Entry stagger, closing and reduced/none in every factory theme. |

These check names identify existing coverage, not a claim that this investigation executed all of them. The focused
attachment and message-bar capture is recorded in `motion-frequent-input-2026-10-24.md`; it does not approve this inventory.

## Verified findings and unfinished corrections

- Message actions moved vertically and attachment expansion exceeded 300 ms. The focused hidden UI run failed first
  (400 ms and a -4 px transform), then passed 5/5 twice after correction. PR 317 contains that correction and the
  independent code/coherence review. Main-screen composition passed before and after; eight image pairs received
  40/40 from a fresh reviewer. Those images do not show hovered actions or expanded attachments. Full details and
  limitations are in `motion-frequent-input-2026-10-24.md`; integration remains unverified.
- The later `.ask .note.err` shorthand restored the long arrival duration despite the earlier shared card override.
  A new native check reproduced 400 ms in Dark Room. The correction uses the short interface token.
- Workshop's 240 ms update entry plus its 72 ms stagger reached 312 ms. A separate new native check reproduced this.
  The correction uses the existing shared 60 ms stagger ceiling.

The latter two checks examine every factory theme and full/reduced/none. Both failed before the corrections, then
both passed after them. The positive run took 288 s including a full-screen-app hold (29.3 s build, 2.2 s UI).
Fresh static review found no remaining blocker. The real What's New screen was captured at 720x480 and 1280x800 in
Workshop: a planted restoration of the old delay reached 312 ms, and the corrected entry reached 300 ms. The fresh
visual reviewer scored both versions 39/40 minimum, with no regression; final screenshots do not judge motion quality.
A pre-existing small-window overlap between a notes heading and the title-bar mark lost one fit point. The later
[window-frame correction](notes-window-frame-2026-10-24.md) records its reproduction, fix and independent 40/40 after
critique. The question-error state was not photographed. Integration and the complete slow walk remain unfinished. Details are in `motion-chrome-timing-2026-10-24.md`; evidence is retained outside caches at
`E:/Documentos/ProjetosIA/faber-evidence/motion-chrome-timing-20261024/`.

## Existing slow-walk machinery

`testkit/src/screen/sentinel.rs` registers panels, permission, composer, text and composition scenarios. Its
`clock_and_slow` verifies the animation clock before `Animation.setPlaybackRate(0.25)`. Panels cover tutorial steps,
running-column open/close at 1440x900, 1000x700 and 720x480, the narrow sidebar, Settings and reduced motion.
Composition covers 1920x1080 and 1280x800.

This is not a complete inventory walkthrough. Remaining menus, Catalog details, native picker behavior and recoverable-error
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
percentages. Permission menus, edge placement, rapid reversal, reduced/none, normal-speed performance, runtime
memory/processor cost, integration and full task-997 approval remain unverified by this focused run. Item navigation,
action activation and destructive-action confirmation were not exercised; external desktop focus was not measured.

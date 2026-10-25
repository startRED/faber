# First-user round: 2026-10-25

Status: bounded second pass and an empty-Projects discovery repair verified locally; integration and full task closure remain unverified. This is dated evidence, not a description of current Faber behavior.

## Scope and method

Task 996 asks for first run, a Chat turn, a project conversation, Catalog and Settings at 720x480 and 1280x800, with Haiku and Codex gpt-6-luna, in pt-BR and English. Findings are separated into blockers, confusion and taste. Evaluator declarations are checked against actual images, available controls and the program's completion receipt.

The earlier matrix has 16 reports: eight default walks and eight recorded Demo-project conversations across both models, languages and sizes. It is historical coverage, not 16 new runs on this repair. Receipt exceptions are retained: Haiku pt-BR/720 default has a report but no normal completion receipt; its project conversation is a retry without a lease receipt; both Codex pt-BR/720 reports lack lease receipts. The old two second-pass pairs do not establish a full current-source repeat.

This continuation ran four targeted walks before the UI repair and four afterward. All eight executions ended normally; that is distinct from the evaluator completing its task. The harness saves controls before asking the evaluator, binds the driver/application/fake CLI at five boundaries, and requires natural driver exit with CLEAN. Runs used the normal shared hidden-app resource guard. No scenario-specific coordinator acknowledgement was received; the recorded interpretation is not an acknowledgement or independent authority approval. No credentials were inspected or real provider account connected inside the test app.

## Findings and disposition

| Surface | Finding | Disposition |
| :--- | :--- | :--- |
| First opening | Some historical reports claimed there was no next action, although Connect was visible. Appearance choices needed scrolling. | Both current Codex/720 default walks reached the home: English at step 6, pt-BR at step 4. The explicit connection prerequisite is not an account-free answer failure. Controlled appearance observations establish reachability of all three choices, not spontaneous discovery. |
| Catalog | Usage guidance is below the first viewport; some evaluators stop before scrolling or returning. | Current pt-BR completed at step 6. English reached its eight-step cap immediately after Back; the next task image still shows the Catalog list before opening Chat. No broken Back control was established. Below-fold prioritization remains discovery friction rather than a proven functional blocker. |
| Chat discovery | Default disconnected mode cannot produce an answer without an AI. The previous focused Chat recipe incorrectly used that same mode. | The harness now separates disconnected discovery from the existing recorded-Chat fixture. A focused pt-BR walk sent the sky question, showed the recorded Rayleigh answer and returned to idle with an empty composer. This is replay evidence, not a live-provider or project-file-editing proof. |
| Empty Projects navigation | At 720x480 the reached drawer showed neither its purpose nor a discoverable Add action. Both models encountered this confusion. | Reuse the existing localized folder-purpose sentence before the illustration and keep the existing Add control visible and inside the list viewport. No second action or changed navigation behavior. The targeted native check failed before the change and passes afterward in all five locales. |
| Settings | Language selection was discoverable. | Both current default walks completed it. This is discovery of the control, not a full locale-behavior audit. |
| Folder selection | Some post-repair evaluators clicked Add and reported no result. | The native folder picker is outside the WebView image/control boundary. These captures do not establish whether it opened, failed, was cancelled or created a project. Do not classify a production picker defect from an unchanged WebView image. |

The drawer intentionally stays open when changing areas; changing that behavior would contradict the existing navigation design. Its visible close control was not evidence of a broken home. No navigation redesign was made.

## Second-pass results

Before repair, Codex English and pt-BR default walks both became stuck at empty Projects; a focused Haiku pt-BR/720 walk also became stuck. A separate Codex pt-BR recorded-Chat walk completed at step 5. English default used an eight-step cap; the other focused/default walks used ten.

After repair, the same focused Projects goal was left unchanged:

| Evaluator | Viewport | Outcome | What the evidence supports |
| :--- | :--- | :--- | :--- |
| Codex, English | 720x480 | Stuck at step 9 after three Add clicks | It first read the newly visible purpose; the later claim of no explanation is contradicted by its own image and text. Native folder selection remains unobserved. |
| Codex, pt-BR | 720x480 | Stuck at step 8 after Add and two waits | It read the purpose and found Add. The unchanged WebView cannot decide the native-picker result. |
| Haiku, pt-BR | 720x480 | Done at step 5 | It explained adding a folder for Faber to work inside, while still expressing uncertainty about that phrase. |
| Codex, English | 1280x800 | Done at step 6 | It explained folders as places where the AI can work. No folder creation was confirmed. |

These are two done and two stuck evaluator outcomes, not four successes. The independently verified repair removes the observed missing purpose/action; it does not establish full task-996 closure or a successful native folder operation. No coaching was added to make the task pass.

## Proof and independent review

The focused check is `shell: empty Projects explains its purpose and exposes Add at 720 by 480`. Tests-only RED failed for the absent purpose. The first candidate still failed because Add protruded above the list viewport. The final empty-only header geometry passed the unchanged check: exact existing purpose, visible bounded Add, localized accessible name, programmatic focus and exactly one call to the existing mocked action, across en, pt-BR, es, fr and de. This is not a full Tab traversal or an actual folder-pick proof.

The independent source review found no blocker and checked C005/C008/C011 with R168/R170. Five npm proofs, formatting and claims passed on the six repair files. The separate harness safeguards passed 26 unit tests; seven failed with tests alone before the corresponding safeguards were applied. Their continuation is saved and queued separately, not claimed incorporated here.

The independent actual review verified all 89 post-repair frozen payloads and original hashes, six source bindings, all 28 original PNGs and controls, four reports and five-stage runtime identities. Narrow paired visual scores rose from 33/40 to 35/40 with no per-line drop; wide after scored 37/40. There is no paired before image at 1280 or standard 1920 collection, so wide non-regression and the full standard composition procedure remain unverified. This is target-specific empty-navigation evidence.

The application and fake CLI stayed unchanged across the four post-repair walks. The first driver legitimately changed during its normal build and stayed stable afterward; subsequent walks used the resulting driver throughout. No claim that the pre-build driver was the executed binary, or that the mixed harness/application checkout is one combined source head.

## Taste, resources and limits

The remaining narrow-state visual losses are low polish: Add is quiet beside New conversation, Projects repeats as an area/group label, and the empty illustration/label extend below the short list viewport. The new purpose and action take priority. One short owner card asks whether to retain that priority or shrink the illustration to fit. No answer has been received and no further redesign is authorized by elapsed time.

The repair adds one short paragraph and changes only empty-state header geometry; no timer, listener, resident process or dependency is added. Its runtime memory/CPU delta was not measured. Protected build-cache storage warnings are not application resource measurements. The queued native suites retain the existing resource limits.

Task 996 remains unfinished: incorporation of the fixes and the native-folder boundary are not certified here, and two post-repair evaluators still declare stuck. Existing Demo-project reports cover recorded conversations only, with the receipt exceptions above. Live-account operation, creating a real project folder, editing project files, normal-speed feel, full keyboard traversal, persistence and the full standard wide photo procedure were not proven by this continuation.

## Evidence locator

Local evidence is kept under `E:/Documentos/ProjetosIA/faber-evidence/`:

- `newcomer-996-exact-acceptance-inventory-20261025.txt`: historical 16-cell coverage and receipt exceptions. Its old pending-work status is superseded only by the actual results recorded here.
- `first-user-safeguards-adoption-20261025/`: exact external-patch adoption, tests-only RED, 26-test GREEN, independent review and saved/queued receipt.
- `newcomer-second-pass-20261025/`: four before-repair walks; exclusive frozen payload counts 61, 45, 21 and 15; `initial-actual-review.txt` and `independent-followup-actual-review.txt`.
- `projects-empty-discovery-20261025/`: native RED, rejected first candidate, final GREEN, source proofs/reviews, four after-repair walks, exclusive frozen payload counts 33, 22, 16 and 18, and `independent-actual-visual-review.txt`.

All frozen payloads are immutable. Execution receipts, screenshots and controls support the bounded claims above; evaluator prose alone is not a product verdict.

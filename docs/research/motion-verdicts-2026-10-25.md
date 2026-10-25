# Motion verdicts: bounded observations and remaining work

Status: unfinished. Task 997 requires a verdict for panels, menus, cards, the composer, the running column, the
sidebar, Catalog and first run. This note reconciles preserved observations; it does not close that task or approve
the current integrated application. No implementation or timing change follows from an unobserved motion.

The rules remain [looks](../design/looks.md#motion), the `faber-ui` checklist and the eight-line `visual-critique`
rubric. A still composition score is not a verdict on continuous movement. Approximate samples are bracketed by
DOM observations, rather than represented as exact video frames. Quarter playback alone does not prove that a
screenshot contains an intermediate animation state.

## Eight required surfaces

| Surface | Bounded verdict from saved observations | Still required |
| :--- | :--- | :--- |
| Panels | Historical Settings reversal preserves focus, cleanup and settled composition: 40/40 at 1920, 1280 and 720 widths in full/reduced/none. Approximate full-motion overlap samples score 38–39/40, with explicit transient deductions. | Other panel paths and continuous normal-speed feel remain unapproved. Historical snapshots do not prove the corrected current tree. |
| Menus | Composer context and Settings permission menus have twelve settled states at 40/40. Four approximate full-motion samples score 38/40 with explicit overlap deductions; the other eight sampled states score 40/40. | The observation is 1280x800/PT-BR. Full entrances used quarter playback; exit used normal wall time because removal is timed. Multi-size and continuous normal-speed motion remain unapproved. |
| Cards | Native parent02 observes Running selection/Send and the same original Thread question folding/fading out at 1280x800/PT-BR/full. All four original images were reviewed: settled, selected, partial exit and complete composition each 40/40. The partial image positively contains the original card. | This supersedes the earlier missed-exit image, not its preserved receipt. Other modes/widths, ordinary-speed feel, complete exit duration and actual answer-payload fidelity remain unapproved. |
| Composer | Parent03 supplies seven independently reviewed 720x480/full endpoint images at 40/40. Separate single-key parent06 observes one native Shift+Enter from two to three lines: 129 interior samples independently match the global quarter clock. | Endpoint composition and one effect's clock do not prove intermediate growth pixels or continuous motion. Earlier rapid-fill parents04/05 were interrupted and remain inconclusive, not disproved by the one-key control. Clear/Send opacity is not a shrink animation; reduced/none and other widths remain unapproved. |
| Running column | Corrected parent03 completes nine cases at 1027 overlay, 1028 docked and 720 interruption across full/reduced/none. All 30 originals were reviewed; close/focus/ghost inertness and suppression paths pass within scope. Static composition is 35/40 with five explicit deductions. | Historical runtime excludes the later reopening correction. Populated/automatic paths, current combined source and continuous normal-speed feel remain unapproved. Static repetition/occlusion deductions are not newly demonstrated product defects. |
| Sidebar | Parent04 completes nine cases and 36 images at 720x480/PT-BR. Focus returns 9/9. Full entry has visible intermediate samples; reduced/none close is observed. Settled-open and reduced/none-closed composition score 40/40; one partial full entry scores 39/40 for moving-edge clipping. | All three full images labelled closed-settled still show the sidebar: the observer waited for class/scrim removal, before transform/opacity settled. Full visible exit and the settled closed raster remain inconclusive. Do not infer a current-product defect from that premature capture label. |
| Catalog | Navigation parent02 completes full/reduced/none, 18 actions and 45 reviewed images at 1280x800/PT-BR: Skills, search, present-card detail, Back and Chat; heading/returned-card focus and sticky Back at three scroll positions pass. Static composition is 36/40 with four deductions. Earlier twelve hover/focus endpoints remain 40/40. | The sampled detail image equals its endpoint; it does not demonstrate intermediate detail motion. Other widths, ordinary-speed continuous movement and a before/after implementation comparison remain unapproved. Sticky Back does not require all detail content to have zero motion. |
| First run | Native parent01 completes six real Back/Skip transitions across full/reduced/none at 1280x800/PT-BR/no-AI. All 14 originals were reviewed: destination heading focus, setup-only persistence and in-memory mode-preview retention pass. Both full outgoing stages are visible. Static composition is 40/40 except three prepared scrolled views at 39/40 for a partly obscured heading. | The blend with the incoming screen and partial style fidelity remain inconclusive; muted pixels do not establish a missing stylesheet or ordinary-speed flash. No restart, Continue, final exit or retry was exercised. Earlier 720 controlled look selection remains reachability evidence, not spontaneous newcomer discovery. |

## Provenance and limits

Evidence is retained outside build caches under `E:/Documentos/ProjetosIA/faber-evidence/`. The dated coverage index
`motion-997-coverage-after-345-20261024.txt` collects the earlier surface summaries.
Its current eight-surface table and later actual-observation addenda supersede the earlier missing-capture rows.
Each result belongs to its own historical source/runtime pins; these are not observations of one combined build.
The later executed parents below ended naturally with a CLEAN marker. Their independent actual-result reports
checked archive and input hashes; that proves preservation and cleanup, not general stability or the current version.

- `settings-reversal-20261024/visual-05/actual-independent-review.txt`: nine observations, 36 PNGs and 53 frozen
  files, with 16 matched input pins. The review's initial 150 ms blocker was withdrawn after checking the conditional
  frequency rule. It is not evidence that every Settings switch must be classified as a frequent interaction.
- `menu-motion-observation-20261024/visual-parent-02/actual-independent-review.txt`: 24 PNGs, 41 frozen files and
  27 matched input pins. Intermediate entry/exit samples are bounded observations, not a normal-speed feel verdict.
- `catalog-motion-observation-20261024/native-04/fresh-visual-score.txt`: twelve individually inspected original
  endpoint images, each scored on all eight rubric lines; no intermediate-frame or whole-Catalog approval.
- `newcomer-996-appearance-observation-20261024/parent-02/actual-independent-review.txt`: nine images, 24 frozen
  files and 19 matched input pins; controlled reachability only, not completion of the first-user task.
- `question-card-motion-20261024/native-parent-02/actual-result-visual-review.txt`: four original images,
  52 frozen payloads and 31 matched input pins. Positive partial original-card pixels; single full-mode scenario only.
- `composer-growth-motion-20261024/current-parent-03/actual-result-review.txt` and its separate
  `fresh-visual-review.txt`: seven reviewed endpoint images, 40 frozen files and 23 matched pins;
  `composer-growth-motion-20261024/single-key-parent-06/actual-result-review.txt`: five images, 48 frozen files
  and 27 matched pins, independently
  recomputed quarter-clock agreement. The clock/cleanup approval is not an intermediate-pixel or motion-quality score.
- `running-motion-observation-20261025/parent-03/actual-result-visual-review.txt`: 30 reviewed original images,
  73 frozen files and 34 matched pins.
  Earlier parent02's 1099 overlay assertion was an observer error: the source threshold with a 288 px sidebar is 1028.
  Its failed archive remains preserved; the corrected parent uses 1027/1028 and checks actual sidebar geometry.
- `sidebar-motion-observation-20261025/parent-04/actual-result-visual-review.txt`: 36 reviewed original images,
  75 frozen files and 30 matched pins. The closed-shot boundary observation records the premature full-mode label.
- `catalog-motion-observation-20261024/navigation-parent-02/actual-result-visual-review.txt`: 45 reviewed
  original images, 90 frozen files
  and 36 matched pins; three motion modes, navigation and controlled scroll, not continuous page-transition approval.
- `first-run-motion-20261025/native-parent-01/actual-result-visual-review.txt`: 14 reviewed original images,
  75 frozen payloads and 44 matched pins; six Back/Skip transitions, with appearance/name/experience uncommitted.
  Setup is saved as skipped; the selected motion preview survives those setup-only saves, not a tested restart.

Settings, menu and the earlier appearance observations used preserved historical geometry binaries. Reported lab memory points
and cumulative CPU counters include automation and do not establish peak/idle budgets or current-source cost.
Their natural cleanup and successful execution receipts do not establish integration or general stability.

## Remaining admissible observations

Do not rerun immutable captures or restore binaries into a build cache. New observations require a coordinated native
window, normal resource guards, truthful source/runtime admission and fresh independent review of actual results.
The earlier rejected composer preparation stays rejected; later successful parents do not retroactively approve it.

The remaining evidence gaps are specific: visible settled full-mode sidebar closure; intermediate composer growth
pixels; the first-run incoming/outgoing blend and partial style fidelity; other required sizes/modes and relevant
automatic paths. A read-only paired computed-style/selector observation could distinguish a first-run style mismatch
from opacity/compositing; current source inspection has not established a cause. Fix only a demonstrated rule break,
not an inconclusive screenshot. No timing or styling change is proposed by this reconciliation.

No surface receives complete task-997 approval here. The corrected current tree, remaining surfaces, resource budgets
and continuous normal-speed movement remain unfinished.

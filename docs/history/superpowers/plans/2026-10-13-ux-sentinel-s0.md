# UX sentinel S0: implementation plan

Spec: [2026-10-12-ux-sentinel-design.md](../specs/2026-10-12-ux-sentinel-design.md), section 12, slice S0. Owner decision
R104 settled the open points: the composer gap is 64 px (shipped); only critical and high findings block a merge, a
stable update or a release; medium ones become tasks; nothing records the owner's screen.

**Exit (from the spec).** The four owner findings fail on today's build or are shown fixed with evidence, every planted
defect fails, and the run adds 90 s at most.

## Shape: reuse the testkit, add two files

The sentinel is a set of scenarios in the existing `screen` suite, so `cargo t` already picks them by changed file
(`ui/` pulls in the screen suite) and every run is already hidden, sandboxed and audited. Nothing new is launched.

| Piece | Where | Notes |
| :--- | :--- | :--- |
| Judging rules | `testkit/src/sentinel.rs` | Pure functions on numbers: teleport, mojibake, composer growth, thread gap, clock. Findings with fingerprints, severity, the baseline gate, the findings file. Unit-tested with synthetic samples, so the planted twin of each rule costs no launch. |
| In-page probe | `testkit/src/probe.js` | Injected over CDP like `locator.js`; samples boxes and opacity every frame and returns raw numbers. No build step, so no `tests/ux/probe.ts` yet (S1 if a bundle is needed). |
| Scenarios | `testkit/src/screen/sentinel.rs` | Four, each one launch: motion, composer and thread, text, and the matrix sweep of the text scan. |
| Known findings | `tests/ux/baseline.json` | Fingerprints open at the base commit. A finding of severity critical or high that is not in it fails the scenario. This is the ratchet and the merge gate; CI already runs the screen suite. |
| Static text scan | `tests/ui/mojibake.check.ts` | Runs in the `ui` suite: every locale string and every source string. |
| Screen hooks | `ui/*.tsx` | `data-screen` on each screen root and `data-surface` on each moving panel, nothing else. |
| Launch switches | `src-tauri/src/main.rs` | The three anti-throttling switches, in test builds only. |
| Docs | `docs/design.md` | A "UX sentinel" part under "Screen tests". |

Cut from S0 (all of section 5 beyond the list, the screen map file, the coverage guard, nightly matrix, AI review,
dogfood, reference bands): they are S1 to S3. The thresholds that S0 needs are constants in `sentinel.rs`.

## Detectors in S0 and their planted twins

| Detector | Measures | Planted twin (injected into the page of the sandbox, never the repository) |
| :--- | :--- | :--- |
| Clock | 1 s transition: at least 45 frames, median under 20 ms, `transitionend` | Synthetic samples with a 100 ms median |
| M1 teleport | Run column docked and over the stage, sidebar over the stage under 900 px, Settings, the tutorial card: box and opacity per frame at quarter speed | `transition: none; animation: none` on the surface |
| M6 reduced | With reduced motion: no running animation moves anything or lasts over 120 ms | A style adding a 400 ms transform animation |
| I1 to I5 | Composer height at 1, 2, 5, 10, 40 lines, cap, shrink after send, controls on the bottom edge, thread end kept | `height: 60px !important` on the field |
| T1 | Rendered text of the screens in all 5 languages, static scan of locales and sources, a title with `› « » ß ç ã` round-tripped through the engine's JSON | A node holding `â€º` |
| L4, L5 | Gap from the last line of the last message to the composer; last item cut by the scroll edge | `padding-bottom: 0` on the thread |

## Steps

1. Commit the spec, then this plan.
2. First code commit: replace the two layout transitions in `ui/styles.css` (`padding-right` on the greeting,
   `height` on the composer field) with motion that costs no layout, and adjust the checks that asserted the old
   transitions without raising any budget.
3. Write each rule and its twin test first, see it fail, then the code. Scenarios run on the base build first to record
   which owner findings fail today.
4. Measure the added run time of the new scenarios and the peak memory of the end-to-end test.
5. Run every proof, push, open a draft PR into `phase-1-base`, wait for CI.

# Catalog detail return visibility, 2026-10-23

Status: bounded UI correction, not first-stable qualification or completion of task 996.

## Trigger and correction

Three small-window newcomer walks scrolled a tool's detail and could not discover the return control above the
viewport. A directed baseline proved that scrolling to the top restored it; it was not an impossible navigation.
The Catalog-only matrix and unedited reports are recorded separately in `catalog-newcomer-audit.md`.

The same existing Back button now sits in an opaque sticky bar across the reading column. It keeps the existing
handler, keyboard behavior, translated label and focus return. There is no second navigation control, dependency,
process, timer, stored setting or animation. This extends the existing Catalog tab entry in the product map.

An initial button-only treatment passed the navigation but obscured fragments of text beside the button. Independent
image review rejected that composition; the full-width opaque bar replaced it before saving the change.

## Executed checks

The new `catalog: return stays reachable after reading a small-window detail` check failed before implementation:
Back was outside the reading viewport. After implementation an additional hit test exposed an isolated-fixture
problem: its fixed root began underneath the test app's caption. Diagnostic output identified `header.top`; the
fixture now uses the same high layer as its existing visual preview. The full native app separately demonstrated
the click, so this fixture correction did not substitute for testing the actual shell.

The final bar passed the regression check. The extended version passed in all five shipped languages, with normal
text and 24px secondary/28px body text. It checks a scrolled detail, complete return-label width, viewport bounds,
hit testing, return to the list and focus on the original card. Enlarged fixture text is not browser zoom proof.

Both Codex gpt-6-luna and Claude Haiku repeated the previously failing 720x480 English task using screenshots and
reachable controls only. Each opened the detail, scrolled, read the conditional-use explanation and returned to the
list without scrolling back up. Independent image review confirmed both sequences, rather than accepting `done`.
No tool was installed or changed. The existing hidden test build was rebuilt from this change before these walks.

## Visual and resource evidence

Fresh independent review scored the 720px before/after 39/40 to 40/40, and 1280px 40/40 to 40/40. The single lost point
was the return disappearing after scrolling. The bar separates navigation from reading without lateral text
fragments. A separate unmodified detached-baseline capture completed the 1920px comparison: 40/40 before and after.
The full detail fits at that size, so it verifies composition, not sticky behavior; the smaller windows verify that.
Cards checked: C006, C008 and C011. The captures use Dark Room and English; other native themes and browser zoom have
not been visually inspected. The automated check covers translated labels and enlarged fixture text separately.

In the directed 1280x800 no-AI flow, private working set of the app and its whole tree was 101.71 MiB at the initial
sample and 143.66 MiB after Catalog loading, reading and return. WebView presence was confirmed in both readings.
Processor time increased from 2281.25 to 3890.625 ms, or 1609.375 ms across the complete flow. These samples include
resizing, loading and interactions: they do not isolate the bar's incremental cost or establish idle qualification.
The 1920px flow ended at 154.20 MiB. No resource threshold or collection mechanism changed.

## Local artifacts

All named logs and reports below are under `E:/Temp/faber-tools/`; their screenshots have separate generated folders
under `E:/Temp/faber-target/catalog-update/faber-test/first-user/`:

- `catalog-return-negative.log`: expected original geometry failure.
- `catalog-return-hit-diagnostic.log`: caption overlap in the isolated test fixture, not a native Catalog failure.
- `catalog-return-fixture-positive.log`, `catalog-return-bar-positive.log`, `catalog-return-languages-positive.log`:
  staged positive checks; only the last contains the final extended language/text coverage.
- `catalog-return-codex-720-en-after.md` (`2026-10-23-MGWTQm`) and `catalog-return-haiku-720-en-after.md`
  (`2026-10-23-zSKmiD`), with matching logs: independent newcomer repeats.
- `catalog-return-bar-1280-report.md` (`2026-10-23-Gj7EA4`) and `catalog-return-bar-1920-report.md`
  (`2026-10-23-vyWReB`), with matching logs: directed complete-shell checks and resource samples, without an AI reviewer.
- `catalog-return-before-1920-report.md` (`2026-10-23-7UfofD`) and matching log: freshly rebuilt baseline capture from
  the unmodified detached checkout `E:/Temp/faber-catalog-return-before-20261023`, before this UI change.

Directed diagnostics reuse a generic report template whose introductory newcomer wording does not apply; their
explicit reviewer labels and recipes identify them as deterministic probes. Evidence does not establish real
provider compatibility, automatic capability choice, installation or a stable release.

Verified copies of the source snapshots, selected before/after images, reports, recipes and logs are retained outside
the mutable build cache in `E:/Temp/faber-tools/catalog-return-frozen-final-20261023/`, with per-file SHA256 receipts.
Runtime executables are not archived or retrospectively bound to individual runs in this visual-evidence bundle.

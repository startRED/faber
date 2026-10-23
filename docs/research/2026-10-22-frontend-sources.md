# Frontend source study, 2026-10-22

Original synthesis for Faber's own UI and the interfaces it creates. This is not a competitor benchmark or a record
of measured app behavior. Source access date: 2026-10-22. No third-party skill body or implementation was copied.

## Findings that change implementation

| Source | Useful finding | Application and limit |
| :--- | :--- | :--- |
| [WCAG 2.2](https://www.w3.org/TR/WCAG22/) | Accessibility has testable criteria, not just visual taste. | Check keyboard, focus, names, contrast, reflow and status. A scan alone cannot certify conformance. |
| [W3C IndexedDB 3 draft, transaction durability](https://www.w3.org/TR/IndexedDB-3/#transaction-durability-hint) | A transaction's default durability follows the browser; strict requests stronger persistence before completion, with an I/O and power tradeoff. | Draft attachments are not a reconstructible cache. Distinguish a committed app-restart proof from an OS-crash or power-loss guarantee; the durability option remains a browser hint. |
| [WAI APG keyboard](https://www.w3.org/WAI/ARIA/apg/practices/keyboard-interface/) | Composite widgets need a predictable keyboard model. | Use native controls or the relevant complete pattern; a role attribute is insufficient. APG is informative guidance. |
| [WCAG target size](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html) | AA uses a 24 CSS px minimum with specified exceptions. | Treat as a floor. Important touch controls may need more space. Do not claim every icon must be 44 px under AA. |
| [Microsoft Fluent layout](https://fluent2.microsoft.design/layout) | Proximity and spacing convey relationship and importance. | Group and align before adding cards/dividers. Its spacing ramp is an example, not a replacement for Faber's tokens. |
| [GOV.UK error message](https://design-system.service.gov.uk/components/error-message/) | A field input error differs from service failure. | Preserve valid input and connect the message to the affected control; do not blame a field for a server outage. |
| [GOV.UK error summary](https://design-system.service.gov.uk/components/error-summary/) | Multiple field failures need an actionable path back. | Use the existing form pattern where relevant, rather than adding a global error panel to every screen. |
| [web.dev INP](https://web.dev/articles/optimize-inp) | Responsiveness includes input delay, processing and presentation. Good INP is <=200 ms at p75 of visits. | Do not relabel a p95 click target as field INP. Profile the delayed phase and test WebView interactions directly. |
| [Emil Kowalski's original motion article](https://emilkowal.ski/ui/7-practical-animation-tips) | Immediate feedback, interruptions and timing change perceived responsiveness. | Use selectively within existing motion rules. Scaling every button or animating every list is not a universal improvement. |
| [Apple HIG layout](https://developer.apple.com/design/human-interface-guidelines/layout) | Platform-specific layout reference. | The fetched page required JavaScript; no detailed recommendation here is attributed to unread page content. |
| [IBM Carbon loading](https://www.carbondesignsystem.com/building-blocks/core/patterns/loading) | Loading cues depend on the operation and known content structure. | Prefer an appropriate inline state; skeletons do not replace action controls or justify fictional progress. |
| [Sara Soueidan on focus indicators](https://www.sarasoueidan.com/blog/focus-indicators/) | Focus styling is part of usability, not optional decoration. | Inspect the indicator against its actual surroundings and preserve it when applying a custom look. Normative levels still come from WCAG itself. |
| [Josh W. Comeau on Grid](https://www.joshwcomeau.com/css/interactive-guide-to-grid/) | Visual arrangement can diverge from source/keyboard order. | Inspect reading and keyboard sequences after responsive grid rearrangement, not just the screenshot. |
| [NN/g recognition and recall](https://www.nngroup.com/articles/recognition-and-recall/) | Context and recognizable choices reduce memory demands. | Clear labels, recent work and contextual choices support discovery; adding every command to the screen does not. |

The Material layout page returned 404 and web.dev's rendering-performance page timed out in this pass. They were not
treated as accessed evidence. A reference link in a skill is a lookup destination, not a claim that its whole site
was read. The research intentionally favors original standards and author guidance over scraped tip lists.

## Blueprint corrections

- The 50 ms local-feedback, 100 ms optimistic-message and 150 ms panel figures are proposed lab budgets, not W3C
  requirements or measured Faber results. Preserve labels and choose fixtures/hardware before evaluating them.
- A pending message is not a durable outbox. Idempotency needs an effect-side contract and recovery tests; a disabled
  button or optimistic bubble cannot prove zero duplication.
- Virtualization, workers, prefetch, content visibility and extra caching are candidates after profiling, not baseline
  features to add everywhere. They can increase memory, break focus/search or produce stale state.
- A new capability such as voice, SSH or scheduling has product, permissions and lifecycle costs. A UX technique
  list does not establish feasibility, source availability or authority for external actions.
- Native desktop and websites use different viewport/input assumptions. Faber's 720 px minimum is not a mobile
  breakpoint for every generated site. Preserve an existing project's layout and visual identity.

## Practical evaluation fixtures

These are acceptance scenarios. The first [exploratory bakery pair](2026-10-22-frontend-evaluation.md) has now run;
the other fixtures remain unexecuted. Keep model version,
instruction set, task, assets, hardware and runtime build recorded. An independent reviewer sees anonymized results.

| Scenario | What the reviewer must look for |
| :--- | :--- |
| Improve a dense project dashboard | Clear primary task; useful density; existing token reuse; no gratuitous hero section |
| Build a small bakery website | Specific hierarchy, real or labeled sample content, purposeful imagery and working navigation |
| Repair an async form | Slow/error/success states, preserved input, no duplicate submission, field vs service error distinction |
| Refine a keyboard-operated split view | Reachable resize, visible/restored focus, minimum readable column and drag alternative |
| Long streaming conversation | Stable reading position, bounded expensive content, recoverable input and truthful status |
| Backend-only change, or a video request | Does not load or impose the frontend workflow unnecessarily |

Record task success and concrete visual findings separately. Program proofs cannot judge taste, and taste votes do
not demonstrate durability, accessibility or resource use. The first pair tied on the independent visual rubric;
its unequal skill consultation and single sample per condition do not establish a causal improvement.

## Follow-up: identity during pagination (2026-10-23)

The official Solid [Index reference](https://docs.solidjs.com/reference/components/index-component) describes reuse
by position; its [For reference](https://docs.solidjs.com/reference/components/for) describes mapping by value
identity. A positionally reused element can represent different content after prepending or windowing. Review
focus, selection and temporary markers by logical record identity when the visible window changes. A blanket
component swap is not a performance argument: object identity and streaming updates also need inspection.

This is guidance for the cross-platform search/reading investigation, not proof of its cause or a mandate to
replace every positional list. The regression reproduction and the eventual fix must supply their own evidence.

## Follow-up: process memory and object lifetime (2026-10-23)

Chrome's original [memory investigation guide](https://developer.chrome.com/docs/devtools/memory-problems)
distinguishes OS memory footprint from the JavaScript heap and recommends examining retained objects and allocation
over time. A collectible frame element is one observation, not proof that the whole process tree returned to its
prior footprint. Keep the existing app memory budget, record the failed scenario's own measurements and distinguish
them from neighboring scenarios. A passing rerun alone does not diagnose an earlier overage or establish a repair.

## Follow-up: identifying the form user's input purpose (2026-10-23)

[WCAG 2.2 SC 1.3.5](https://www.w3.org/TR/WCAG22/#identify-input-purpose) is normative: defined input purposes for
information about the user must be programmatically determinable where the technology supports this. The W3C
[Understanding explanation](https://www.w3.org/WAI/WCAG22/Understanding/identify-input-purpose.html) is informative;
it explains why a label or broad input type alone may be insufficient and how HTML `autocomplete` conveys purpose.
It explicitly separates exposing the purpose from the browser choosing to autofill. This is not a blanket rule
for every input or records about other people. Synthetic name/email/phone attribute checks establish only their
markup contract; they do not establish AA conformance, autofill behavior or a repeated causal gain from the skills.

# Frontend source study, 2026-10-22

Original synthesis for Faber's own UI and the interfaces it creates. This is not a competitor benchmark or a record
of measured app behavior. Source access date: 2026-10-22. No third-party skill body or implementation was copied.

## Findings that change implementation

| Source | Useful finding | Application and limit |
| :--- | :--- | :--- |
| [WCAG 2.2](https://www.w3.org/TR/WCAG22/) | Accessibility has testable criteria, not just visual taste. | Check keyboard, focus, names, contrast, reflow and status. A scan alone cannot certify conformance. |
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

These are acceptance scenarios for future paired runs, not executed model evaluations. Keep model version,
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
not demonstrate durability, accessibility or resource use. Do not claim the new skills beat the baseline before
these paired runs exist.

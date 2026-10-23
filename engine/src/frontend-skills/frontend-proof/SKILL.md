---
name: frontend-proof
description: Verify implemented UIs through real flows, keyboard, responsive visuals and resource evidence. Use before frontend delivery or for a requested UI audit.
---

# Frontend proof

Prove the requested task, then judge the visible result. A passing compilation, accessibility scan or screenshot alone
cannot establish that the interface works. Choose the checks affected by the change; do not run every check below on
an unrelated one-line edit. Respect the project's stronger proof and review rules.

## Establish what can actually be checked

Read the request and affected flow. Name its acceptance conditions, route and supported viewport/input environments.
Locate the existing checks and run them. Use the project's browser or hidden app harness where available; never launch
a visible window when the host forbids it. If there is no runtime, missing sign-in or another blocked prerequisite,
state that limitation and perform the checks still possible. Never invent a running preview or test result.

Keep commands, runtime errors, screenshots and measurements in the project's existing evidence location. Do not put
secrets, private pages or personal inputs in screenshots or logs. Synthetic fixtures should be visibly synthetic.

## Walk the affected task

Use representative content and a real click/keyboard sequence from entry to outcome. Confirm what changed and what
remained intact. For a changed asynchronous flow, include relevant cases from this list:

| Case | Evidence to seek |
| :--- | :--- |
| Slow completion | Immediate, honest feedback; usable input; no fake progress or success |
| Failure and retry | Text and attachments preserved; actionable error; one intended operation |
| Repeated action | Double activation does not create duplicate work; disabled state is not the only protection |
| Stale response | Old search/navigation results do not replace the current request |
| Cancel and late completion | Cancellation does not revert to a pending or successful state on an older event |
| Navigation away and back | Correct draft, reading anchor, selection, panel and tab state where promised |
| Reload or restart | Durable state restored from confirmed storage; memory-only state described honestly |
| New output while reading | Read line stays in place; new output remains discoverable without forced scrolling |

Do not perform destructive or external operations just to test them. Use approved fixtures or a sandbox. A persistence
check that never reloads proves only in-memory continuity; a retry check must also inspect the resulting record count.

## Keyboard and accessibility

- Reach each changed action without a mouse. Follow Tab/Shift+Tab order, activate with the expected keys and verify
  Escape, arrows and focus return for custom widgets. Background shortcuts must not intercept text entry or IME.
- Check accessible names, roles, labels, headings, errors and state announcements. Do not announce every streaming
  token. Use an automated scan when available, and record its scope; it cannot prove WCAG conformance.
- Inspect visible focus, including beneath sticky bars, overlays and a growing composer. Confirm modal background
  inertness only for actual modals. A panel opened by background work should not take the person's focus.
- Inspect contrast in affected themes and native forced-colors mode. Test status comprehension without color.
- Exercise reduced motion, larger text/zoom and a non-drag alternative for any new draggable control. Use the native
  screen reader on supported systems when available; record it as untested when it was not exercised.

## Content, layout and visual critique

Capture the affected before/after states at a wide viewport, the smallest supported viewport and a content-driven
intermediate width. Use project sizes where defined; include phone width only when the deliverable supports phones.
In Faber itself include its 720 px minimum rather than claiming it is a phone application.

Use long headings, an unbroken identifier, large counts, empty content, missing media and the longest supported locale.
Check normal and enlarged text. Verify page overflow, clipped actions, code/table scrolling and composer visibility.

Judge each changed screen on these questions; point to the screenshot and concrete defect instead of giving vague
adjectives or an unsupported score:

1. Is the main task clear, with secondary controls visually subordinate?
2. Does grouping, alignment and spacing express real relationships?
3. Are typography, contrast and reading width comfortable with real content?
4. Is the result coherent with existing screens and the requested visual identity?
5. Does it use imagery and motion purposefully without obstructing the work?
6. Do the empty, waiting, error and populated states all remain usable?

Apply the project's numeric rubric if it has one. The writer must not invent an independent reviewer or approve their
own visual work. An optional preference is not a defect. Report a blocker only with a reproducible user problem or a
written requirement it breaks. A scoped fix is not permission to redesign neighboring screens.

## Performance and resource evidence

- For affected hot paths, measure interaction-to-next-paint and frame times on the same hardware, fixture and build
  mode before/after. Report sample size and p95/p99 only when there are enough observations; never infer them from
  one run. Distinguish local lab measurements from field INP (p75 of visits, <=200 ms is good).
- Exercise representative long content if rendering, logs or lists changed. Inspect DOM growth and released handlers,
  timers, object URLs, iframes and WebViews after closing. Do not virtualize merely to improve a synthetic count.
- If lifecycle or motion changed, measure memory and CPU while idle and hidden, separately from active AI processes.
  Retain the existing project budget. Do not claim zero cost from source inspection or a quiet-looking screen.
- Run affected project checks and the required final checks. Keep the receipt and report actual failures; a larger
  bundle or failed resource budget cannot be waived by an attractive screenshot.

## Report and stop

Say what works, the evidence that supports it and what was not exercised. Fix a demonstrated in-scope defect and rerun
the checks it affects. Report adjacent issues separately. Ask only for decisions or prerequisites that change the
result; neither this skill nor a screenshot grants authority to publish, install or send anything.

## Primary references

- [WAI keyboard patterns](https://www.w3.org/WAI/ARIA/apg/practices/keyboard-interface/)
- [WCAG 2.2](https://www.w3.org/TR/WCAG22/)
- [Target size and its exceptions](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html)
- [INP measurement](https://web.dev/articles/inp)
- [Rendering pipeline](https://web.dev/articles/rendering-performance)
- [GOV.UK error summary](https://design-system.service.gov.uk/components/error-summary/)
- [Sara Soueidan on focus indicators](https://www.sarasoueidan.com/blog/focus-indicators/)

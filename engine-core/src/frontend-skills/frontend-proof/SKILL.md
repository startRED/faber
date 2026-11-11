---
name: frontend-proof
description: Use before saying a page or screen is done, or for a UI audit: look at it at desktop and phone size with no visible window, then verify flows.
---

# Frontend proof

Prove the requested task, then judge the visible result. A passing compilation, accessibility scan or screenshot alone
cannot establish that the interface works. Choose the checks affected by the change; do not run every check below on
an unrelated one-line edit. Respect the project's stronger proof and review rules.

## Look at it before you say it is done

Never hand over a page or screen you have not seen, and never ask the person for a look you can take yourself. Use
the first that applies:

1. The browser tool this session offers, at a desktop and a phone size.
2. The project's own screenshot or end-to-end harness.
3. An installed Chromium browser (Chrome, Edge, Brave) run headless, which opens no window. Find the program, then:
   `<browser> --headless --disable-gpu --hide-scrollbars --user-data-dir=<temp>/profile --screenshot=<temp>/desktop.png --window-size=1280,900 <file:/// address or local URL>`
   A headless window does not go narrower than about 500px: a smaller `--window-size` crops the picture instead of
   narrowing the page. For the phone, write a temporary wrapper page next to the pictures,
   `<body style="margin:0"><iframe src="<address>" width="390" height="844" style="border:0"></iframe>`, and take its
   picture with `--window-size=390,844`. Raise both heights (such as 2600) to see a long page whole.
   Keep pictures and profile in the system's temporary folder, never in the project or the person's own browser
   profile, and delete them when finished.

Read both pictures as the person would. Is anything clipped, overlapping or off-screen? Is every text readable on
the phone, including text inside drawings? Does the wide layout use its width with intent, or is it a phone column
floating in emptiness? Is each requested element present? Fix what you see and look again; stop after two rounds
unless a defect remains. Text under 14px on the phone, in a drawing too, is a defect to fix, not a note to report.
What a picture shows is what the person gets: never explain a defect away as an effect of the capture. A panel
that stops short of the window, a gap or a cut edge is fixed, then seen fixed. Look once more after any later
change, however small.

A picture does not prove behavior. To exercise a page's script with no window, make a temporary copy of the page
that ends with a short script which clicks and types through the flow and writes what it observed into the page,
then read it back: `<browser> --headless --disable-gpu --user-data-dir=<temp>/profile --virtual-time-budget=3000 --dump-dom <address>`.

Finish, do not offer. Run every check you can run yourself before the closing message; never end by asking
whether to run one. Ask the person only for a decision that is theirs. The request is done when it is met, seen
and exercised; state plainly anything left unexercised. If no browser can run under the current permissions, say so.

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
For a slow asynchronous path designed to finish, exercise its actual completion as well as waiting and cancellation.
A separate instant-success mode does not prove that the slow operation finishes, releases controls or preserves the
right result. A deliberately indefinite wait is checked against its own contract, including cancellation or recovery.

## Keyboard and accessibility

- Reach each changed action without a mouse. Follow Tab/Shift+Tab order, activate with the expected keys and verify
  Escape, arrows and focus return for custom widgets. Background shortcuts must not intercept text entry or IME.
- Check accessible names, roles, labels, headings, errors and state announcements. Do not announce every streaming
  token. Use an automated scan when available, and record its scope; it cannot prove WCAG conformance.
- For changed forms collecting information about the form user, inspect supported input-purpose metadata against
  each applicable field's actual meaning, including missing or mismatched HTML `autocomplete` tokens. Visible labels
  and input types alone are insufficient; whether the browser chooses to autofill is not the pass condition.
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

Illustrated demos use three to five short beats, real product tokens and recognizable controls. Label synthetic
content; scene clicks never prove product behavior. Use an installed Stageshow skill only for these demos. Copy needed
kit/examples into the task workspace before edits or dependency installation; never modify a verified Catalog snapshot.
Keep demo dependencies outside the app. Compose narrow layouts separately and provide a complete reduced-motion still.
Illustration crops and word limits must not hide real app information or controls.

For a changed transition or animated demonstration, retain a visual rehearsal of the actual interaction: its opening,
intermediate images, settled outcome and, for a loop, the return to its opening. Include a narrow supported layout,
reduced motion, interruption and replay where relevant. Pair images with the sampling trace so a reviewer can locate
a jump instead of guessing from one settled screenshot.

Treat missing images, missing samples, sampling gaps or a timeout before settling as incomplete evidence. Inspect the
structured report, coverage and command exit status together; a printed approval line alone is insufficient. A clean
automated report still needs visual judgment. Report the browser and build actually exercised, and distinguish
illustrated scene checks from clicks through the real application.

- For affected hot paths, measure interaction-to-next-paint and frame times on the same hardware, fixture and build
  mode before/after. Report sample size and p95/p99 only when there are enough observations; never infer them from
  one run. Distinguish local lab measurements from field INP (p75 of visits, <=200 ms is good).
- Exercise representative long content if rendering, logs or lists changed. Inspect DOM growth and released handlers,
  timers, object URLs, iframes and WebViews after closing. Do not virtualize merely to improve a synthetic count.
- If lifecycle or motion changed, measure memory and CPU while idle and hidden, separately from active AI processes.
  Retain the existing project budget. Do not claim zero cost from source inspection or a quiet-looking screen.
- Run affected project checks and the required final checks. Keep the receipt and report actual failures; a larger
  bundle or failed resource budget cannot be waived by an attractive screenshot.
- After restoring a negative control or switching sources, verify the executable or served bundle actually exercised.
  A source checksum alone does not establish its build identity. Invalidate stale artifacts through the build system
  and retain the running build's identity before treating its result as proof of the corrected version.

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

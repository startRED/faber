# A visual answer's frame sometimes stays on the first page after a version switch

Area: Visual answers
Files: ui/VisualAnswer.tsx, tests/ui/visual-answers.check.ts
Depends: none
Size: about 80 changed lines

Why: the `ui` check "visual answers: web changes apply on the next opened version, keep one frame and do not reload an open
page" failed on CI in 4 pull requests in eight hours (task 005) and twice in 20 local runs on 2026-10-30. Task 005 gave each of
its waits its own message, and both local failures named the same stage: after `.visual-prev` the screen said "1 of 2", the
check's `api.visualPage` wrapper had asked for and received the second page (2 asked, 2 answered), and still the frame never
left the first page: one frame load, one `DOMContentLoaded` report, `loaded.url()` still the first page's address, for the
whole 15 s (a page made never to report still counts two frame loads after the switch, so one load means no second load
happened). So the version switch reaches `VisualPane` and the native side answers; the navigation the screen starts
(`target.location.replace(next.url)` in the `page` effect of `ui/VisualAnswer.tsx`) either does not run or is dropped by
WebView2. A person would see the counter on one version and the page of the other. A probe that switched version right
after the frame appeared, before its first page reported, passed 5 of 5, so "the click lands before the first load" is not
the cause by itself.

Build: find which of the two it is. The check's failure line now also says whether the first page had reported before the
click. Record in the effect (test-visible, for example a counter on the frame element's dataset) whether `location.replace`
was called for the new address, and run the check in a loop under load until it fails again. If the effect never ran, fix
the effect; if it ran and the navigation was dropped, make the pane notice that no load followed within a few seconds and
navigate once more or show "visual.failed" (never a stale page under a new counter).

Done: the check passes 20 times in a row under load; a temporary change that drops the navigation shows the failure line
naming it, and the product then does not leave a stale page. One review by the other vendor.

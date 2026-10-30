# The visual answers `ui` check names the wait that timed out and waits on events, not a 30 ms poll

Area: none
Files: tests/ui/visual-answers.check.ts
Depends: none
Size: about 60 changed lines

Why: the `ui` check "visual answers: web changes apply on the next opened version, keep one frame and do not reload an open
page" (`tests/ui/visual-answers.check.ts`, line 90) failed in 4 runs on 4 pull requests within eight hours:
visual-answers-names run 40965537512 (2026-10-29 22:23), pr-441 run 40978270541 and pr-446 run 40980708009 (2026-10-30
00:13 and 00:36), pr-464 run 41015041220 (06:36; the same pull request landed on its next attempt with no change to this
check). Only the first of them is about visual answers. The message is always "the visual answer appeared before the
deadline", the default text of `until` (line 68), so it does not say which of the four waits in the check timed out (the
first iframe, the navigation after `.visual-prev`, the one after `.visual-next`). Of the three logs read, two failed after
4.1 s and one after 15.1 s, which a single 15 s `until` does not explain, so the wait that fails may not be the one assumed.
The check has a helper for this, `loadedVisual(host).why()` (reports, frame loads, insertions, frame src), and passes it to no
`until` of this check. The waits poll `settle()` (30 ms of real clock) for a message an iframe posts from its own document,
which on WebView2 can be slow on a loaded runner.

Build: give each `until` of this check its own message with `loaded.why()` and the values it compares (`frame.src`,
`loaded.url()`, `prepared.get(...)?.live`), replace the poll by waiting on the events that matter (the iframe `load`, the
`message` the page posts) with the 15 s only as the guard, and keep the assertions as they are. Then run the check in a loop
(`cargo t --filter "visual answers: web changes"`, 20 times, with another `cargo t` running to load the PC) until one fails and
read which stage it was; fix that cause in the product or the check. If no run fails in 40 loops, say so, land the diagnostics
and leave the rest: the next failure then names itself.

Done: the loop above passes 20 times in a row; a temporary change that makes the page never post its message turns the failure
into a line that says which wait and what `why()` saw. One review by the other vendor.

# Reading

How conversation text reads and keeps its place, in Chat and Projects.

- The conversation reads in a column of 760 px (`--col-w`, shared by the thread and the message box) with a line height
  of 1.68 (`--read-lh`) and 14 px between paragraphs, so a 720 px window still has about 640 px of text. A code block has
  a bar with its language and a copy button for that block. Selecting words in an answer floats "Ask about this" over
  them (menu motion), and it puts them in the message box as a quote. Scrolled up, a "Jump to latest" button floats at
  the foot of the thread and scrolls down smoothly.
- Returning to a conversation, including after a restart, keeps the same message at the same reading offset even when
  earlier text or pictures changed height. Search still opens its result, keeping its focus and brief outline if scrolling
  loads an earlier page. Pagination restores its viewport after reconnecting focus, so any geometry change caused by
  focusing a reused row is included in the correction. Reading above the tail never follows new words;
  scrolling by hand immediately changes the reading point. A removed message falls back to its surviving neighbour.
  This correction is instant at every motion level and uses the existing message window, not a second scrolling view.
- "Jump to latest" first selects the conversation's newest message window, then scrolls to its end; it never stops at
  an intermediate loaded page. Reaching that tail removes the old reading position from storage.

## Verification

The hidden Chat and Projects screen scenarios in `testkit/src/screen/reading.rs` each use 160 synthetic messages and no AI. Each closes the
first app and requires its process tree, including WebView2, to exit before enlarging an earlier saved message and
relaunching the same profile. It checks the real message id and viewport offset, native wheel scrolling, the latest
message and forgotten tail position, the unchanged conversation file, and screenshots at 1280 and 720 px.

The existing tree memory limit remains 400 MB. Report this fixture's sampled tree memory and CPU over its measured
post-launch flow intervals; these are not isolated anchor overhead, broad latency results or a zero-cost claim.

### Measured fixture (Windows, 2026-10-23)

The original Chat 160-message scenario passed with motion set to none and no AI. The first app and WebView2 tree ended before
the same profile reopened in another native process; a larger earlier message preserved message 7 at its -24 px offset.
The 1280 and 720 px captures, native wheel gesture, one-click jump to message 159 and removal of the stored position passed.

| Measurement | Result |
| :--- | :--- |
| Sampled app and WebView2 tree peak | 165.9 MB, below the unchanged 400 MB limit |
| CPU across the two post-launch scripted intervals | 2,141 ms over 2,094 ms, 102.2% of one core |
| Whole native scenario, including launches and shutdown | 20.5 s |

CPU covers navigation, scrolling and screenshots on this fixture; it excludes app launches and the closed-profile interval.
It is a flow measurement on a busy local PC, not isolated feature overhead or a broad interaction latency result.
The disabled-cache mutation failed only after the real relaunch; its exact source bytes were restored before the green run.

### Projects restart follow-up (Windows, 2026-10-23)

The Projects case seeds one synthetic project and opens its conversation through the real area, project and conversation
controls. After closing the app and WebView2 tree, it checks the same project identity and message 7 at its -24 px offset
in another native process using the same profile. Earlier content grows while closed; 1280 and 720 px captures, a native
wheel gesture, one jump to message 159, forgotten tail metadata and the unchanged conversation file are checked too.

A temporary Projects-only omission of the loaded reading cache failed after the second real launch at the viewport
assertion. Restoring the exact original app source made both Chat and Projects pass, with zero failed/not run/skipped.

| Follow-up fixture | Sampled tree peak | CPU / measured post-launch flow | Scenario duration |
| :--- | :--- | :--- | :--- |
| Chat | 168.0 MB | 1,281 / 1,455 ms | 8.2 s |
| Projects | 176.5 MB | 1,250 / 1,484 ms | 8.0 s |

These are single busy-PC flows including navigation, scrolling and screenshots, not isolated overhead, a performance
comparison or a zero-cost claim. The project is a synthetic local folder with no AI; this does not cover remote projects,
other operating systems, smooth motion or a screen reader. This verification-only extension changes no app code.

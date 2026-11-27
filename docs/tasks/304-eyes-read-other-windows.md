# Eyes: the AI reads other windows as text

Area: Named app control
Files: winproc/src/eyes.rs, winproc/src/lib.rs, engine-core/src/eyes.rs, engine/src/engine/eyes.rs, engine/src/engine.rs, engine-drivers/src/helper.rs, engine/src/sharing-sources.txt, docs/design/eyes.md, docs/product-map.md
Depends: the-key-for-eyes-and-hands
Size: about 900 changed lines
State: built text-only, with the second review's defects fixed; waiting for a third review (R146).

Why: plan item E1, owner's answers 1 and 2 (25/11/2026). The proof of concept (`winproc/examples/eyes.rs`,
`docs/dev/eyes-proof.md`) measured four things:
- UI Automation trees came back in under 0.4 s, without moving the focus or the pointer.
- PrintWindow pictures worked for classic and custom-drawn windows.
- Chromium pages gave nothing off screen.
- Electron pages appeared only after an accessibility client asked.

Build:

- Move the reading parts of the example into `winproc`, where unsafe calls stay:
  - list the top-level windows;
  - read a bounded UI Automation tree, with depth and node caps as in the example.

  Request accessibility from Electron windows before reading them.
- Add a conversation tool, `look`, with two actions: `windows` and `tree`. It is offered only while the key is on, never
  for a program on the out-of-reach list, and in ask modes it asks before each new window. Nothing ever takes the focus,
  moves the pointer or shows a window (C006).
- Windows only. Elsewhere, the tool is not offered.

Leave out: any action in another window. That is hands, which come after the signed installer. Pictures of a window
(Windows.Graphics.Capture, PrintWindow) come later, as their own task: an embedded window of a program out of reach
can show inside an allowed one's pixels, and capture calls have no deadline of their own, so they need their own design
and review.

Done:
- Unit tests of the tree caps, the text format and the out-of-reach filter. No real windows in unit tests.
- A measured run of the example suite on the maintainer's PC, before and after, recorded in the design page.
- The memory and processor cost of a tree.
- Permissions and privacy: two reviews from different vendors (R146).

State (2026-11-27): two review rounds (R146) found defects; this round makes the first delivery text only (the picture
path, its argument, strings and tests are gone) and fixes the rest of the second round's list, each with a test: the
list is read again for every process met inside a tree and once more under the engine's lock before the answer goes
out, and an answer holding a program now out of reach is dropped; a name without `.exe` keeps its program out, and a
name keeps out every program it starts; the accessibility request runs on a thread the read leaves at its deadline; a
window is bound to its thread and class too, and a UWP frame names its app only when exactly one other process draws in
it; the key's epoch is taken when a call comes; the card saves the program, never the window's title; a CLI's request
under a window card's name is refused. Not landed: it waits for a third review.

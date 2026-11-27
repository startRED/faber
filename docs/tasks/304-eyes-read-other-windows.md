# Eyes: the AI reads other windows as text, and as a picture only when needed

Area: Named app control
Files: winproc/src/eyes.rs, winproc/src/lib.rs, engine-core/src/eyes.rs, engine/src/engine/eyes.rs, engine/src/engine.rs, engine-drivers/src/helper.rs, engine/src/sharing-sources.txt, docs/design/eyes.md, docs/product-map.md
Depends: the-key-for-eyes-and-hands
Size: about 900 changed lines
State: built (look tool, winproc reading, ask per window, measured before and after); waiting for the two reviews (R146).

Why: plan item E1, owner's answers 1 and 2 (25/11/2026). The proof of concept (`winproc/examples/eyes.rs`,
`docs/dev/eyes-proof.md`) measured four things:
- UI Automation trees came back in under 0.4 s, without moving the focus or the pointer.
- PrintWindow pictures worked for classic and custom-drawn windows.
- Chromium pages gave nothing off screen.
- Electron pages appeared only after an accessibility client asked.

Build:

- Move the reading parts of the example into `winproc`, where unsafe calls stay:
  - list the top-level windows;
  - read a bounded UI Automation tree, with depth and node caps as in the example;
  - take a picture (Windows.Graphics.Capture first, then PrintWindow), with a zoom to a region.

  Request accessibility from Electron windows before reading them.
- Add a conversation tool, `look`, with three actions: `windows`, `tree` and `picture`. It is offered only while the key is
  on, never for a program on the out-of-reach list, and in ask modes it asks before each new window. The tree comes first;
  a picture is taken only when asked for. Nothing ever takes the focus, moves the pointer or shows a window (C006).
- Windows only. Elsewhere, the tool is not offered.

Leave out: any action in another window. That is hands, which come after the signed installer.

Done:
- Unit tests of the tree caps, the text format, the zoom region and the out-of-reach filter. No real windows in unit tests.
- A measured run of the example suite on the maintainer's PC, before and after, recorded in the design page.
- The memory and processor cost of a tree and of a picture.
- Permissions and privacy: two reviews from different vendors (R146).

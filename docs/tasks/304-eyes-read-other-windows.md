# Eyes: the AI reads other windows as text, and as a picture only when needed

Area: Named app control
Files: winproc/src/eyes.rs, winproc/src/lib.rs, engine-core/src/eyes.rs, engine/src/engine/eyes.rs, engine/src/engine.rs, engine-drivers/src/helper.rs, docs/design/eyes.md, docs/product-map.md
Depends: the-key-for-eyes-and-hands
Size: about 900 changed lines

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

State (2026-11-27): built on the branch `night/eyes-parked` and fixed after two review rounds (R146), parked, not landed.
The second round still finds: the final delivery does not re-read `programsOutOfReach`, and a tree's child filter keeps
the list read at its start; an entry without `.exe` never matches; a picture checks only the outer window's owner, so an
embedded window of a blocked program can show; `PrintWindow` and `AccessibleObjectFromWindow` have no deadline once past
the hung check; a handle recycled inside the same process keeps its consent; the key's epoch is read after the window
lookup; the pending card stores the full caption before consent; `is_self_change` omits the `faber.eyes.*` cards; and
the final-reply race and same-process reuse have no test.

State (2026-11-27, evening): a third round on the branch `night/eyes-text-only` dropped pictures (text through
accessibility only) and fixed the nine points above. Its reviews (Codex, Antigravity) still find: the final check runs
before the reply enters the driver queue; a handle reused by the same process, thread and class keeps its consent; a UWP
frame is named by the class of its one foreign child, not its package identity; a tree remembers an allowed program by
process id, and the Chromium warm-up contacts child windows without an owner check; looking up one window reads every
top-level caption, blocked programs included, and can hang on Faber's own window; the accessibility read can outlive its
deadline; lower-casing breaks the prefix match for some letters (final sigma); and a lookup can finish on the next turn
or after the turn ended. The fifth and seventh points ask for a design change: read windows in a separate process that
can be stopped. That is the owner's call before a fourth attempt.

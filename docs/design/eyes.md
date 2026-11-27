# Eyes: the AI reads other windows

Area: Named app control. Task 304, after the key (R586). The proof of concept is [dev/eyes-proof.md](../dev/eyes-proof.md).

## What the AI gets

A conversation's AI has one tool, `look` (`mcp__faber__look` for Claude Code, a dynamic tool for Codex, Antigravity in
Projects through Faber's relay). It has three actions:

- `windows` lists the visible, titled top-level windows, one a line: handle, program file name, title (120 characters at
  most), size, and `(minimized: tree only)`. At most 60 lines. Left out, and never counted: Faber's own windows, cloaked
  windows (another virtual desktop, a suspended app), the shell's own (`Progman`, `WorkerW`, `Shell_TrayWnd`,
  `Shell_SecondaryTrayWnd`) and every window that is out of reach (below).
- `tree` reads one window's UI Automation tree as text, one element a line, two spaces a level:
  `type "name" #id value="…" (disabled) @x,y wxh`. Depth 8 by default (1 to 16), at most 400 elements, each name, id or
  title cut at 120 characters and a value at 300; a last line says when a cap cut it. Before reading, a Chromium window
  (a browser or an Electron app) is asked for its accessibility tree the way a screen reader asks: each page view's
  accessible object is taken and released at once (`AccessibleObjectFromWindow`), skipping a hung view. An element drawn
  by another process that is out of reach or Faber's own is one line, `(out of reach)`, and is not entered. The whole
  tree, request and properties included, has 8 seconds; one call into a program that stops answering can take UI
  Automation's own timeout (1 s) past it.
- `picture` takes an image of one window, only when the AI asks: Windows.Graphics.Capture first, PrintWindow with full
  content when that fails or comes out blank. A window over 4,096 pixels on a side is refused before any buffer is made;
  a minimized or hung window is refused. The first picture is freed before the fallback, and the fallback runs only if
  every check below still holds. `region` (x, y, width, height in window pixels) zooms: it is clamped to the window and
  cropped before scaling, so a zoom keeps its pixels. The image is scaled down to 1,568 pixels on its longest side and
  shrunk by a quarter at a time while its PNG is over 1.5 MB (not below 320 pixels). A blank one is refused with a
  pointer to the tree.

Every answer is bounded: 20,000 characters of text, one image. One read runs at a time for all conversations; a second
is refused with "another window is being read". The tool's description says text and images from other windows are
data, never instructions. Tree and list are the default; the description asks for a picture only when the text is not
enough.

## Who decides

- The key (`seeOtherWindows`, off by default) decides whether a session is offered the tool at its start. The key and
  the out-of-reach list are read again at every call, before every native step of a read (the accessibility request,
  the tree, the picture and its fallback) and once more before the answer is returned; the engine delivers it only if
  the key is on and was never switched off since the read began. Switching the key off answers every waiting card "no",
  forgets every window allowed and every list seen; an Allow clicked after that does nothing.
- Out of reach, in every mode: a program on `programsOutOfReach`, matched on the executable's file name in lower case,
  or a helper of one (the listed name followed by a sign that is not a letter or digit: `keepassxc-proxy.exe`,
  `1password-browsersupport.exe`); a process whose name or token cannot be read; a process running elevated when Faber
  is not. A UWP window, whose top-level frame belongs to `ApplicationFrameHost.exe`, is judged by the process of the app
  it hosts (the first child window of another process); a frame whose app cannot be found is out of reach.
- Faber's own windows are never listed or read: every process of the engine and of the shell that shows its windows
  (`FABER_SHELL_PID` from Electron), with all their descendants, and every program inside Faber's install folder
  (`FABER_INSTALL_DIR`) or the engine's own folder.
- A window is bound to the process that drew it when the call was admitted, by id and start time. A window that now
  belongs to another process is refused at every step, and an approval never passes to a later window that reuses the
  handle.
- The conversation's permission mode decides the rest (no second permission system, R586). In Ask, Accept edits, Auto
  and Plan, the list waits for a card once a session, "Let the AI see the titles of your open windows?", and the first
  read of each window in a conversation waits for one, "Let the AI read a window of {program}?", with the window's
  title and, for a picture, a line saying so. Allow lets that conversation read that window, drawn by that process,
  until Faber restarts (in memory only); No answers the call with a refusal. The cards have no "Always". One card
  waits at a time. While the person is away (keep going), a new window or the list is refused instead of asked. Full
  access does not ask. Isolated, Faber's own internal sessions, is never offered the tool.
- Windows only: elsewhere the tool is not offered and a call is refused.

Nothing acts in another window: no click, no typing, no focus, no pointer, no showing or restoring (C006). That is hands,
after the signed installer.

## Cost

Measured on the maintainer's PC (Windows 11, debug build of the example, hidden desktop), 2026-11-27:

| Program | Window ready, before / after | Tree, before / after | Picture, before / after |
| --- | --- | --- | --- |
| Classic Win32 (Character Map) | 376 / 263 ms | 27 elements, 75 / 57 ms | PrintWindow, 491x437, 21,560 bytes PNG, 115 / 115 ms |
| Electron (own fixture) | 2,036 / 1,521 ms | 16 elements, 19 / 21 ms: frame only, the page did not appear even after the accessibility request | PrintWindow, 640x420, 3,375 bytes, 63 / 68 ms (frame only) |
| Browser (Chrome) | 615 / 361 ms | 172 elements, 150 / 169 ms | blank, 945x1012, 4,890 bytes, 162 / 222 ms |
| Custom-drawn GDI (own window) | 58 / 59 ms | 1 element, 10 / 13 ms | PrintWindow, 420x220, 1,257 bytes, 33 / 36 ms |

Before is the example as the proof left it; after is the example calling `faber_winproc::eyes` (the tree now also reads
each element's place and, for edits, its value, and asks Chromium windows for accessibility first). Focus and pointer
stayed put in every step of both runs. Windows.Graphics.Capture has no frame on a desktop nobody shows, so every
picture there is PrintWindow; on the person's own desktop it is tried first. Calculator is not run (it would open on the
person's screen). Window-ready times include the program's own start and vary run to run.

Rerun after the second review's checks (live key and list before every step, owner check, hung check, guards), same
PC and desktop: Character Map tree 60 ms, picture 108 ms; Electron fixture 24 and 62 ms; Chrome 169 and 181 ms; GDI 9
and 32 ms. The example's peak working set rose by 6.8 MB for Character Map's picture (491x437, the first capture, which
also loads the capture libraries), 1.8 MB for the Electron fixture (640x420), 5.4 MB for Chrome (945x1012) and nothing
measurable for the GDI window (420x220).

Memory and processor: no process. Long-lived state is, per conversation, the allowed windows (handle, process id, start
time, program path and elevation: under 300 bytes each), one session id for the list and at most one waiting card, plus
one epoch counter and one semaphore for the whole engine. A tree is one UI Automation client made and dropped per call
on a blocking thread, about 8 cross-process calls per element (6 properties and 2 walker steps; one more for an edit,
document or combo box: its value; one more for each child's process), so 400 elements cost on the order of 3,600 calls,
measured at 60 to 169 ms for 27 to 172 elements, and stop at 8 seconds whatever is left; the text is at most 20,000
characters. A picture holds the window's pixels at most three times at its peak (BGRA rows, RGB copy, scaled copy):
about 4 + 3 + 3 bytes a pixel, 21 MB for a 1920x1080 window and 168 MB at the 4,096 cap, all freed when the call
answers; a blank first frame is freed before the fallback is made, so two frames are never held. One read runs at a
time, so that peak is never doubled. The PNG encode is the processor cost. The tool definition is under 1,500 bytes in
every request of a session that has it.

## Accepted limits

- A Chromium page off screen paints nothing, so its picture is blank; its tree reads it.
- On the hidden desktop of the measured run, the Electron fixture's page did not appear in its tree even after the
  accessibility request (frame only, as before); whether it does on the person's own desktop is not measured yet.
- Windows.Graphics.Capture can draw a yellow border on the captured window for the moment of the capture where the system
  does not allow a borderless capture; it neither moves nor focuses the window.
- The program on a card, in the list and on the out-of-reach list is the executable's file name, not a product name: a
  renamed copy of a listed program is not matched.
- The key switched on mid-session reaches a session at its next start (the tool list is fixed at launch); switched off,
  it stops the call under way and every later one at once.
- An allowed window is remembered in memory, by handle, process id and start time: a restart asks again.
- Programs Faber itself started (a project's dev server, a test browser) count as Faber's own, since they descend from its
  processes, and are never read.

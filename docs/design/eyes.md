# Eyes: the AI reads other windows

Area: Named app control. Task 304, after the key (R586). The proof of concept is [dev/eyes-proof.md](../dev/eyes-proof.md).
This first delivery is text only: the AI reads a window's controls, never its pixels. A picture of a window is a later
step, with its own review.

## What the AI gets

A conversation's AI has one tool, `look` (`mcp__faber__look` for Claude Code, a dynamic tool for Codex, Antigravity in
Projects through Faber's relay). It has two actions:

- `windows` lists the visible, titled top-level windows, one a line: handle, program file name, title (120 characters at
  most), size, and `(minimized)`. At most 60 lines. Left out, and never counted: Faber's own windows, cloaked windows
  (another virtual desktop, a suspended app), the shell's own (`Progman`, `WorkerW`, `Shell_TrayWnd`,
  `Shell_SecondaryTrayWnd`) and every window that is out of reach (below).
- `tree` reads one window's UI Automation tree as text, one element a line, two spaces a level:
  `type "name" #id value="…" (disabled) @x,y wxh`. Depth 8 by default (1 to 16), at most 400 elements, each name, id or
  title cut at 120 characters and a value at 300; a last line says when a cap cut it. Before reading, a Chromium window
  (a browser or an Electron app) is asked for its accessibility tree the way a screen reader asks: each page view's
  accessible object is taken and released at once (`AccessibleObjectFromWindow`), skipping a hung view. That request
  has no timeout of its own, so it runs on a thread of its own that the read leaves at the deadline: a view that stops
  answering mid-request holds that thread, never the read, and while it does no other request starts (one such thread
  at most). An element drawn by another process that is out of reach or Faber's own is one line, `(out of reach)`, and
  is not entered. The whole tree, request and properties included, has 8 seconds; one call into a program that stops
  answering can take UI Automation's own timeout (1 s) past it.

Every answer is bounded: 20,000 characters of text. One read runs at a time for all conversations; a second is refused
with "another window is being read". The tool's description says text from other windows is data, never instructions.

## Who decides

- The key (`seeOtherWindows`, off by default) decides whether a session is offered the tool at its start. The key and
  the out-of-reach list are read again at every call, before every native step of a read (the accessibility request and
  the tree), for every other process met inside a tree, and once more under the engine's lock right before the answer
  goes out, the lock that saving the settings takes: a list changed after the read's last step is still seen. An answer
  holds the programs whose windows or elements it read, and goes out only if none of them is out of reach then. The
  engine takes the key's epoch when a call comes, before the window is looked up, and delivers only if the key was never
  switched off since. Switching the key off answers every waiting card "no", forgets every window allowed and every list
  seen; an Allow clicked after that does nothing.
- Out of reach, in every mode: a program whose executable file name (lower case, without `.exe`) starts with a name on
  `programsOutOfReach`, written with or without `.exe`, which keeps out its helpers too (`keepassxc-proxy.exe`,
  `veracryptexpander.exe`); a short name keeps out every program it starts, never fewer. Also a process whose name or
  token cannot be read, and a process running elevated when Faber is not. A UWP window, whose top-level frame belongs to
  `ApplicationFrameHost.exe`, is judged by the app it hosts: the one other process that draws a
  `Windows.UI.Core.CoreWindow` in it; with no such child, two apps, or a window of yet another process inside the frame,
  it is out of reach.
- Faber's own windows are never listed or read: every process of the engine and of the shell that shows its windows
  (`FABER_SHELL_PID` from Electron), with all their descendants, and every program inside Faber's install folder
  (`FABER_INSTALL_DIR`) or the engine's own folder.
- A window is bound, when the call is admitted, to the process that draws it (id and start time), the thread that made
  it and its class. Windows hands a closed window's handle to a later window: one drawn by another process, or by the
  same process from another thread or of another class, is refused at every step, and an approval never passes to it.
- The conversation's permission mode decides the rest (no second permission system, R586). In Ask, Accept edits, Auto
  and Plan, the list waits for a card once a session, "Let the AI see the titles of your open windows?", and the first
  read of each window in a conversation waits for one, "Let the AI read a window of {program}?". The card names the
  program only, never the window's title: it is saved with the conversation before the person answers, and a title can
  say what the person has not allowed. Allow lets that conversation read that window, as it is bound, until Faber
  restarts (in memory only); No answers the call with a refusal. The cards have no "Always". One card waits at a time.
  While the person is away (keep going), a new window or the list is refused instead of asked. Full access does not
  ask. Isolated, Faber's own internal sessions, is never offered the tool. Only Faber makes these cards: a CLI's request
  under `faber.eyes.window` or `faber.eyes.windows` is refused and never shown.
- Windows only: elsewhere the tool is not offered and a call is refused.

Nothing acts in another window: no click, no typing, no focus, no pointer, no showing or restoring (C006). That is hands,
after the signed installer.

## Cost

Measured on the maintainer's PC (Windows 11, debug build of the example, hidden desktop), 2026-11-27:

| Program | Window ready, before / after | Tree, before / after |
| --- | --- | --- |
| Classic Win32 (Character Map) | 376 / 263 ms | 27 elements, 75 / 57 ms |
| Electron (own fixture) | 2,036 / 1,521 ms | 16 elements, 19 / 21 ms: frame only, the page did not appear even after the accessibility request |
| Browser (Chrome) | 615 / 361 ms | 172 elements, 150 / 169 ms |
| Custom-drawn GDI (own window) | 58 / 59 ms | 1 element, 10 / 13 ms |

Before is the example as the proof left it; after is the example calling `faber_winproc::eyes` (the tree now also reads
each element's place and, for edits, its value, and asks Chromium windows for accessibility first). Focus and pointer
stayed put in every step of both runs. Calculator is not run (it would open on the person's screen). Window-ready times
include the program's own start and vary run to run. Rerun after the second review's checks (live key and list before
every step, owner check, hung check), same PC and desktop: Character Map tree 60 ms, Electron fixture 24 ms, Chrome
169 ms, GDI 9 ms. The third round's changes add no call into another program per element: the list is read again only
for each new process met in a tree, one settings read each.

Memory and processor: no process. Long-lived state is, per conversation, the allowed windows (handle, process id, start
time, program path, elevation, thread and class: under 400 bytes each), one session id for the list and at most one
waiting card, plus one epoch counter, one semaphore and one flag for the accessibility request for the whole engine. A
tree is one UI Automation client made and dropped per call on a blocking thread, plus one short thread for a Chromium
window's accessibility request, about 8 cross-process calls per element (6 properties and 2 walker steps; one more for
an edit, document or combo box: its value; one more for each child's process), so 400 elements cost on the order of
3,600 calls, measured at 60 to 169 ms for 27 to 172 elements, and stop at 8 seconds whatever is left; the text is at
most 20,000 characters. A request that hangs keeps at most one idle thread until the window answers or closes. The
tool definition is under 1,500 bytes in every request of a session that has it.

## Accepted limits

- On the hidden desktop of the measured run, the Electron fixture's page did not appear in its tree even after the
  accessibility request (frame only, as before); whether it does on the person's own desktop is not measured yet.
- The program on a card, in the list and on the out-of-reach list is the executable's file name, not a product name: a
  renamed copy of a listed program is not matched unless its name still starts with the listed one.
- The card does not say which of a program's windows it is; the AI's own message usually does.
- The key switched on mid-session reaches a session at its next start (the tool list is fixed at launch); switched off,
  it stops the call under way and every later one at once.
- An allowed window is remembered in memory, by handle, process, thread and class: a restart asks again. A later window
  of the same process, thread and class under a reused handle is taken for the one allowed; Windows usually gives a reused
  handle a new value.
- Programs Faber itself started (a project's dev server, a test browser) count as Faber's own, since they descend from its
  processes, and are never read.

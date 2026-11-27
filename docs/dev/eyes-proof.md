# Eyes proof of concept

The reading parts now live in `winproc/src/eyes.rs` (window list, tree, the Electron accessibility request) and the
example calls them; the product uses them through the text-only `look` tool ([design](../design/eyes.md)). Pictures are
a later step: the example no longer takes them, and the picture column of the results below is the proof's, kept for it.

The Windows-only `faber-winproc` example lists top-level windows, prints a UI Automation tree, and
invokes or sets a control by its exact name or automation ID. It measures elapsed time and compares the foreground
window and pointer position before and after each command. A tree defaults to depth 8 and stops at 400 nodes.

Run the five fixtures on an interactive Windows desktop with Edge or Chrome installed. Pass an Electron executable.
The example builds the Electron fixture and creates fresh profiles for it and Edge in a temporary directory.

```powershell
cargo run -p faber-winproc --example eyes -- suite eyes-results --electron node_modules/electron/dist/electron.exe
```

The suite creates an invisible desktop, launches each fixture in its own kill-on-close job, and waits up to 15 seconds
for a matching window owned by that job. On Windows 11 it uses Character Map to avoid the packaged Notepad launcher.
Calculator is skipped unless `FABER_EYES_PACKAGED=1` is set: a packaged app starts through Windows' activation broker
on the person's own desktop, whatever the hidden desktop says, so it would open on their screen. Run it only in an
isolated Windows session.

The hidden desktop thread runs UI Automation. The calling thread measures foreground and pointer position on its
original desktop. No step activates or restores its target.

The suite writes `eyes-results/results.md`. The tree is printed to the console. The custom GDI
fixture intentionally exposes no editable control. A successful Invoke call records provider acceptance.
A Set call reads the value back for up to half a second; when UI Automation's write does not land and the element has
a window of its own, it sends `WM_SETTEXT` and checks it with `WM_GETTEXT`, and says which method worked.

To inspect a fixture manually, run `list` and then pass a handle from its output to the other commands.
Use only windows you opened for the experiment.

```powershell
cargo run -p faber-winproc --example eyes -- list
cargo run -p faber-winproc --example eyes -- tree 0x123456 --depth 4
cargo run -p faber-winproc --example eyes -- invoke 0x123456 "Eyes button"
cargo run -p faber-winproc --example eyes -- set 0x123456 "Eyes input" "Faber Eyes proof"
```

## Results

Measured on 2026-11-26 on the maintainer's PC (Windows 11, Portuguese), debug build, hidden desktop. Focus and mouse
stayed where they were in every step of every program. Windows.Graphics.Capture never worked there (a window on a
desktop nobody shows has no composed frame, `unsupported capture size 0x0`), so every picture is PrintWindow.

| Program | Window | UI Automation tree | Picture (PrintWindow) | Background action |
| --- | --- | --- | --- | --- |
| Classic Win32 (Character Map) | 313 to 471 ms | 27 nodes, 69 to 83 ms | right, 491x437, 21.6 KB, 122 ms | set text: ValuePattern did not take it; `WM_SETTEXT` did, 2.5 s |
| Modern Windows (Calculator) | not run | - | - | - (opens on the person's desktop, see above) |
| Electron (own fixture) | 2.0 to 2.9 s | 16 nodes, window frame only: the page's button and box never appear | frame only, page dark, 640x420, 3.4 KB | invoke button: no element |
| Browser (Chrome; Edge is not installed) | 619 ms | 172 nodes, page controls included, 234 to 367 ms | blank, 945x1012 | set text: ValuePattern, 2.1 s |
| Custom-drawn GDI (own window) | 57 to 60 ms | 1 node (the window) | right, 420x220, 1.3 KB, 39 to 47 ms | none possible: nothing to address |

Recommendation for E1 (eyes) and E2 (hands):

1. Read with UI Automation first: it worked for every program it could reach, in under 0.4 s, without focus.
2. A picture in the background needs a composed window: PrintWindow serves classic and custom-drawn windows, but a
   Chromium page (browser or Electron) paints nothing off screen, so E1 reads those pages by their tree, not by sight.
3. Act through patterns, then window messages (`WM_SETTEXT` took the classic box UI Automation would not); never input.
4. Electron apps expose their page only when an accessibility client asks first: E1 must request it and measure again.
5. Packaged apps cannot be moved to a hidden desktop: E1 works on the windows the person already has, read-only.

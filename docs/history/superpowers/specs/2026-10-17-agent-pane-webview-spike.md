# Agent pane: the cost of a second webview (spike)

Status: spike done, 2026-10-17. Recommendation: **go** with step 7 of "Watching the agent work" (roadmap.md), as a
Tauri child webview in Faber's one window, with one change to how it leaves the screen: it is moved out of the
window's area, never hidden with `hide()`.

## The question

The roadmap ("Cost first") asks for this before Faber's own browser and app pane is built: a hidden child webview,
driven from outside, loads a page and takes a small picture after each of 20 actions. Are the pictures stale or blank,
what does the whole app tree cost in memory and processor against the 400 MB test (R125), and does any window flash or
take the focus while another app is in front?

## How it was measured

- `src-tauri/examples/pane-spike-host.rs`: one Tauri window holding two child webviews, a stand-in for Faber's
  screen and, on request, the pane. They share one WebView2 browser process, as they would in Faber. The pane loads
  another site than the screen, so it gets its own renderer. The window is either never shown, or shown at
  -32000,-32000 like Faber's hidden test builds. Build: `cargo build -p faber --example pane-spike-host --features
  pane-spike` (the feature turns on Tauri's `unstable`, which child webviews need; nothing else builds it).
- `testkit/examples/pane-spike.rs`: starts the host on the testkit's hidden desktop (or, for the focus runs, on the
  person's desktop with the window never shown), drives the pane over the DevTools port with the testkit's own client
  (`cdp.rs`, which gained `connect_where` to pick the pane's page), and measures with `proctree::private_mb` and
  `cpu_ms` (Task Manager's private working set and processor time of the whole tree, through WMI). The host runs in a
  job, so it never outlives the driver.
- The 20 actions cycle click, type, scroll and navigate. Each one repaints a square in the page's corner with a colour
  of its own; the picture (`Page.captureScreenshot`, 800 x 600 PNG) is fresh when that pixel has the step's colour,
  stale when it has an earlier one, blank when it is white or black. A picture that is not fresh is retried for 3 s.
- A watcher polls every 20 ms for a window of the tree on the person's desktop that a person could see, or that holds
  the foreground, and ends the app at the first one. It detects a flash; it cannot prevent one. What prevents it is
  the host's own window settings (never shown, not focused, no taskbar button), which the run tests.
- One run also opens a real site (en.wikipedia.org/wiki/Web_browser) for 5 s, for the memory of a real page.
- Debug build of the host, Windows 11, one PC (the maintainer's), two runs per row (one per round) unless noted.

## Numbers

Two rounds of every mode, an hour apart. Ranges cover both rounds; the processor column is from the second round,
which measures over the real interval between readings (the first divided by the sleep alone and read 0-2 points
higher).

| Window / pane | Fresh first pictures | Picture time (median / max) | Pane added, peak | CPU while acting | Seen or focused |
| :--- | :--- | :--- | :--- | :--- | :--- |
| Shown off screen, pane in view (3 runs) | 20/20 each | 49-57 ms / 59-98 ms | +30-31 MB | 13.5% of a core | - |
| Shown off screen, pane `hide()` | 0/20 both rounds (every picture timed out) | 2 s timeout | +19-25 MB | 6% | - |
| Shown off screen, pane moved out of the window | 20/20 each | 52-54 ms / 75-85 ms | +26-30 MB | 10% | - |
| Never shown (3 runs) | 20/20 each | 45-54 ms / 52-88 ms | +26-30 MB | 8% | - |
| Never shown, throttling switches off | 20/20 each | 40-52 ms / 56-100 ms | +25-28 MB | 10% | - |
| Shown, then minimized | 20/20 each | 40-45 ms / 49-194 ms | +26-29 MB | 9% | - |
| Shown, then hidden to the tray | 20/20 each | 45-52 ms / 101-172 ms | +26-31 MB | 10% | - |
| Person's desktop, never shown (4 runs) | 20/20 each | 45-49 ms / 52-261 ms | +29 MB (the two without a site) | 10-13% | none |
| Then Wikipedia for 5 s (off screen, never shown, person's desktop; 5 runs) | 20/20 each | - | +68 to +82 MB | 10-13.5% | none on the person's desktop |

Memory, for the whole tree (host, browser, GPU, utility and renderer processes):

- The stand-in screen alone: 70-79 MB. Opening the pane adds 14-22 MB at once; the peak during the 20 actions is
  25-31 MB over the start; a real site brings it to 68-82 MB over the start (138-161 MB in all).
- After the pane closes: 7-12 MB stays above the start after the test page and 19-26 MB after Wikipedia (the shared
  browser process keeps its caches), the same at 5 s and at 15 s. When the app ends, its whole tree is gone within
  0.5-9 s (0.7-0.8 s in the second round).
- Against R125: Faber idles near 110 MB with its real screen. Adding what the pane cost here gives an estimate of about
  190 MB with the pane on a real site, under half of the 400 MB test. It is a sum, not a measurement of Faber with
  the pane; the screen suite's memory check (the same private working set) measures that once step 7 exists. One live
  pane at a time (the roadmap's rule) keeps it there.

Processor:

- Idle with the pane open: 0-0.6% of one core, no more than without it (0.3-2.1%).
- Acting: 6-14% of one core over 6-11 s, with a picture after each action (about 2-3 actions and pictures a second,
  slowed a little by the memory readings between them). The pictures are about 12 KB each and take about 50 ms. A
  renderer that ends during the window drops out of the sum, so these are floors.

Pictures and actions:

- No stale and no blank picture in any mode except `hide()`. With `hide()` every picture timed out, and every
  navigation stalled (5 of 20 actions failed). The likely cause, not measured here: WebView2 stops painting a webview
  whose controller is hidden, and the capture waits for a frame that never comes.
- A never-shown, minimized or tray-hidden window does not stop the pictures. `Page.captureScreenshot` asks for a frame
  of its own, so a fresh picture proves that the capture works, not that the page was painting on its own. For the
  pane's log that is the part that matters. The throttling switches the test builds use
  (`--disable-renderer-backgrounding` and the others) changed nothing.

Windows and focus:

- On the person's desktop no window of the tree could be seen and none took the foreground, in both runs (the watcher
  looked every 20 ms through both runs, from start to exit).
- Windows calls three of the tree's windows visible, and none of them can be seen: two input-indicator windows of no
  area, and tao's 16 x 16 event window, layered and transparent, which every Tauri app keeps, Faber included. The
  watcher reports them apart and does not count them.

## Recommendation

Go, with these rules for step 7:

1. The pane is a child webview of the one window (`Window::add_child`, Tauri's `unstable` feature), on the same
   WebView2 environment, so it shares the browser and GPU processes. No second window, no second environment.
2. It leaves the screen by moving out of the window's area (`set_position` far outside), never by `hide()`, which
   stops its pictures and its navigation.
3. Pictures come from `Page.captureScreenshot` over the pane's own DevTools session. They are fresh whatever the
   window does: never shown, minimized or hidden to the tray.
4. The pane closes when nobody uses it. Closing gives most of it back; the 7-26 MB the browser keeps is the price of
   having opened it once.
5. The DevTools port that drove this spike is a test tool. The shipped pane needs a driving channel that no other
   program on the PC can reach: WebView2's own `CallDevToolsProtocolMethod` from the shell, or a port with a secret.
   Step 7 decides which; this spike did not measure it.

Not measured here, for step 7 to check: a release build (the host was a debug build, though the browser processes,
which dominate, are the same), Faber's real screen beside the pane (the stand-in is lighter), a game actually running
in front (the person's desktop had whatever app was in front), focus changes shorter than the 20 ms watcher tick, and
pages heavier than Wikipedia.

## Reproduce

```bash
cargo build -p faber --example pane-spike-host --features pane-spike
cargo build -p faber-testkit --example pane-spike
target/debug/examples/pane-spike.exe "$(pwd -W)/target/debug/examples/pane-spike-host.exe" target/pane-spike/run offscreen
```

Flags: `offscreen`, `pane-hidden`, `pane-parked`, `window=minimize|hide-window` (with `offscreen`), `no-throttle`,
`site=URL`, and `real-desktop` (never shown only; it refuses `offscreen`). Each run writes `summary.json` and the
pictures next to it.

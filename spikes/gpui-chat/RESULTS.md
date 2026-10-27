# GPUI chat measurement spike

## Environment and build

- Windows 11, rustc 1.99.0 (b940084d7 2026-09-28), cargo 1.99.0 (5f94df478 2026-08-27).
- `gpui-component` 0.8.1 and `gpui-kit` 0.8.1 from crates.io, using their pinned `gpui-pre` 0.3.10. The `syntect` feature is enabled.
- Release profile: thin LTO. The four successive build attempts took 215.5 s, 219.1 s, 8.4 s and 75.3 s. The first two compiled dependencies but failed on spike integration errors; the third succeeded and the fourth rebuilt after the window/capture fix. These are incremental attempts, not three independent clean builds.
- Final `gpui-chat.exe`: 26,868,224 bytes (25.62 MiB). `target/`: 3,538,400,679 bytes (3.30 GiB), including check and release artifacts. A WebView-enabled executable could not be built, as explained below.
- The requested `E:\Temp\faber-gpui\tmp` did not exist. Cargo was invoked with the requested environment; no directory was created there. All crate build output was kept in this spike's `target/`.
- `node generate.mjs` produces the two fixtures. `node verify-fixtures.mjs` checked all 1,000 messages' IDs, timestamps, roles, text, assistant metadata and absence of tools, and checked that the small fixture contains the first two messages.

## Measurements

All measurements used the optimized executable. `measure` created a hidden desktop and launched the window using Faber's `desktop::launch` with `CREATE_NO_WINDOW`. It waited until no `cargo.exe` or `rustc.exe` existed before each launch, checking every 30 seconds. Values are whole-process-tree private working set from Faber's `proctree::private_mb`, in MiB; they do not include GPU memory. CPU is process-tree CPU time divided by wall time, expressed as a percentage of one core. Idle CPU was sampled for 10 seconds. Non-streaming memory was sampled after a five-second settle; streaming memory was sampled after the replay ended. Screenshots and UI Automation inspection were performed in the first run of each applicable state.

| State | Private working set runs (MiB) | Median (MiB) | Idle CPU runs (%) | Median (%) |
| --- | --- | ---: | --- | ---: |
| Two messages | 19.40, 16.13, 16.05 | 16.13 | 4.22, 0.00, 0.00 | 0.00 |
| 1,000 messages, bottom | 19.94, 19.14, 19.14 | 19.14 | 0.31, 0.16, 0.78 | 0.31 |
| 1,000 messages, top | 20.41, 19.56, 19.67 | 19.67 | 0.00, 0.00, 0.00 | 0.00 |
| After streaming | 22.23, 21.78, 21.84 | 21.84 | 0.00, 0.00, 0.00 | 0.00 |
| WebView | Not available | Not measured | Not available | Not measured |

Streaming CPU was 16.23%, 10.32%, and 13.99%; median **13.99% of one core**. The first streaming run included the mid-stream screenshot and accessibility inspection. The stream was assembled from text deltas in `tests/fixtures/claude/chat.jsonl`, repeated to roughly 4,000 words with Markdown and a code block, and emitted over 600 updates. Completion markers were 20,447, 20,450 and 20,418 ms after the app's timer began.

Process creation to the first-frame marker for the small conversation was 465, 357 and 373 ms; median **373 ms**. The marker is detected by polling, so this includes polling and scheduling delay. The file cache was not flushed or otherwise controlled; a preliminary launch had already read the executable, so none of these runs can be identified as a controlled cold-cache run. First-frame times written by the app itself were 301, 319 and 330 ms. In the top-scroll runs, the second frame markers were 369, 360 and 365 ms from the app timer.

## Rendering and accessibility

- The hidden desktop produced usable `PrintWindow(PW_RENDERFULLCONTENT)` captures: `out/idle.png`, `out/bottom.png`, `out/top.png`, and `out/mid-stream.png`. The captures were visually inspected. The bottom and top captures show the last and first messages respectively; the mid-stream capture shows an incomplete growing reply.
- Markdown bold, list items and fenced code blocks are visible. The library's selectable Markdown view and syntax-highlighting feature are configured, but selection, Ctrl+C and actual syntax colors were not independently exercised or verified.
- The composer is the component's multiline `Textarea`. IME input was not exercised on the hidden desktop.
- The list uses GPUI `list` and variable-height `ListState`. The first and last rows rendered on demand with 1,000 messages; the captures and memory numbers do not by themselves prove all scrolling cases.
- The measurement executable relaunched itself on the same hidden desktop and walked the raw UI Automation tree. `out/idle.uia.txt`, `out/bottom.uia.txt`, `out/top.uia.txt`, and `out/mid-stream.uia.txt` record Name, Value and Text values. The tree contained window controls but no message text in those fields. Narrator was not run; the tree is the accessibility check.
- The published `gpui-component` 0.8.1 package has no WebView/Wry module or feature. Consequently `--webview` reports unavailable; no Wry version, WebView screenshot, child-process memory or WebView executable size was measured.

## Problems and workarounds

The first preliminary launch wrote a first-frame marker, but the inspection selected a zero-size internal window and returned an empty UI Automation node. Another build also began during that trial. Its 18.21 MiB sample is excluded from the results; `out/initial-trial.txt` and `out/initial-trial.uia.txt` preserve it. The final executable sets explicit window bounds, and the inspector selects the largest nonempty window. The final captures show that this produced a rendered conversation.

The window executable refuses to open unless its thread is on a `faber-test-` desktop. No visible window or console was opened. Raw measurements and build logs are in `out/`; `node summarize.mjs` calculates medians from the measurement file. The target directory was retained.

## Round 2

The window now calls `Theme::change(ThemeMode::Dark, None, cx)` after initialization. The component library defaults to light mode even though the spike's containers were manually painted dark. That made the textarea white and installed a light code-highlight theme. The `syntect` feature and `js` language mapping were already present; the updated `out/top.png` shows differently colored JavaScript tokens, and `out/idle.png` shows the dark composer. This is a spike setup fix, not proof of selection or clipboard behavior; the clipboard was not touched.

Measurements below are new runs of the corrected dark-theme build, not the first-round build above. The private working set covers the process tree; GPU counters cover only the window process, summing all matching GPU instances for each counter. The dedicated and shared figures are separate allocations, not included in private working set. The first run also performs screenshot/UI Automation inspection; the other two do not. No `cargo.exe` or `rustc.exe` was running before each launch. Full samples are in `out/round2-measurements.txt`.

| State | Private working set median (MiB) | GPU dedicated median (MiB) | GPU shared median (MiB) |
| --- | ---: | ---: | ---: |
| Two messages | 27.64 | 18.22 | 1.14 |
| 1,000 messages, bottom | 30.72 | 18.22 | 1.25 |
| 1,000 messages, top | 31.69 | 18.22 | 1.25 |
| After streaming | 33.48 | 24.68 | 2.15 |

The search found no release `faber.exe` under the workspace's `target`. The existing `target/debug/faber.exe` is 47,458,816 bytes (45.26 MiB), a debug build and therefore not a release-size comparison. `target/faber-test/debug/faber.exe` is 51,564,032 bytes (49.18 MiB), also a debug/test build. No Faber build was started.


### WebView experiment

The upstream checkout is `longbridge/gpui-kit` main at commit `c8917c3661ed1090870f403c312b39e0d8a62e0b`. It contains `crates/webview/Cargo.toml` and `crates/webview/src/lib.rs`, plus `examples/webview/`; its WebView wraps `lb-wry` 0.53.3 and WebView2 on Windows. The experiment adapts that example in `gpui-kit-source/examples/webview/src/main.rs` and its manifest, built from this pinned checkout as a separate `target/release/webview.exe`. It uses the ordinary GPUI backend and sets `GPUI_DISABLE_DIRECT_COMPOSITION=true`, as the upstream Windows example does. Its WebView2 profile is inside `out/webview-profile`. The original chat binary and its dependencies remain separate; run the hidden-desktop harness with `target/release/measure.exe --webview`.

The local HTML appeared in the specified 700 by 400 logical-pixel region. `out/webview.png` is a `PrintWindow(PW_RENDERFULLCONTENT)` capture from the hidden desktop. The red GPUI overlay extends across the region's right edge: the portion outside the WebView appears, while the portion over the page is covered by the native WebView. Thus this Windows configuration does not keep GPUI overlays above the page. This conclusion is limited to this backend and configuration, not every possible GPUI or WebView2 rendering mode.

Three runs, sampled after five seconds with the processes still alive, gave these medians. GPU memory sums matching instances across the live window-and-WebView2 process tree; the window-only values are also given. The private working set likewise sums the process tree, including the `msedgewebview2.exe` descendants documented in `out/webview-tree-*.txt`. Samples and idle CPU are in `out/webview-measurements.txt`; the first run includes the screenshot/UI Automation inspection. The GPU counters are separate from the private working set and need not represent all system-wide WebView2 allocations.

| Measurement | Runs | Median |
| --- | --- | ---: |
| Tree private working set (MiB) | 86.77, 85.01, 82.89 | 85.01 |
| Tree GPU dedicated (MiB) | 42.85, 38.34, 38.34 | 38.34 |
| Tree GPU shared (MiB) | 2.84, 2.78, 2.78 | 2.78 |
| Window GPU dedicated (MiB) | 21.48, 18.22, 18.22 | 18.22 |
| Window GPU shared (MiB) | 0.98, 0.95, 0.95 | 0.95 |
| Idle CPU, one core (%) | 0.00, 0.31, 3.59 | 0.31 |

The WebView executable is 19,268,608 bytes (18.38 MiB). The corrected chat executable is 26,873,856 bytes (25.63 MiB). These are different standalone programs: the WebView demo does not include the chat, Markdown or syntax-highlighting workload, so the size difference is not the incremental cost of adding a WebView to chat. All windows and inspection helpers ran on `faber-test-` desktops; no clipboard interaction was performed. The requested temporary directory was created.

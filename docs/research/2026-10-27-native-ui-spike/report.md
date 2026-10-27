# Native UI spike: the chat screen in GPUI, measured

Date: 2026-10-27. Status: done; the spike code was not landed. Question: would a native Rust screen (GPUI, Zed's
framework, with the GPUI Kit components) beat the current SolidJS screen enough to change the direction R459 set?

## What was built

A throwaway crate (`spikes/gpui-chat/`, 305 lines of Rust, kept out of the workspace and out of `phase-1-base`; its
code stays on the branch `spike/gpui-chat-code` for a later re-run) with one window: a sidebar stub, one conversation read from a Faber JSON file through the engine's own `model::Conversation`,
a virtualized variable-height list (GPUI `list`), messages drawn by GPUI Kit's markdown view, a multiline composer,
and a reply replayed token by token at 30 updates a second from `tests/fixtures/claude/chat.jsonl` (about 4,000 words,
20 s). The thousand-message conversation has exactly the shape of the screen check `seed_thousand_messages`
(`testkit/src/screen/app.rs`), so both screens drew the same text.

Versions: rustc 1.99.0, `gpui-component` and `gpui-kit` 0.8.1 (crates.io), `gpui-pre` 0.3.10, release build with thin
LTO. The window ran only on the runner's hidden desktop (`testkit/src/desktop.rs`); the measurement used Faber's own
`proctree::private_mb` (private working set of the process tree, as Task Manager counts it). Three runs per state,
median shown. Screenshots were taken with `PrintWindow`.

## Measurements, same PC, same day

The current screen was measured on the same PC with the existing screen checks (`cargo t --filter thousand` and
`--filter "with no CLI running"`, base `5cbd1720`); CI's numbers from the three newest runs that day are beside them.

| | GPUI spike | Current screen, this PC | Current screen, CI |
|---|---:|---:|---:|
| Idle, small conversation | 28 MB + 18 MB GPU | 121 MB (full motion, dust on) | 105–107 MB (92–95 reduced motion) |
| 1,000 messages open | 31 MB (32 MB after scrolling to the top) + 18 MB GPU | 202 MB, opened in 857 ms | 140–143 MB, opened in ~213 ms |
| After a 4,000-word streamed reply | 33 MB + 25 MB GPU | not measured | not measured |
| Processor while streaming | 14% of one core | not measured (input/scroll/resize load: 122%) | |
| Processor idle | 0–0.3% | 8% of one core with the dust on | |
| Process start to first frame | 373 ms (app's own marker 301–330 ms), file cache not controlled | no recorded number | |
| Executable | 25.6 MB | | Windows installer 5.8–6.1 MB |

Memory is the private working set of the process tree; the GPUI column adds the window's dedicated GPU memory
(`GPU Process Memory` counters; shared GPU memory stayed at 1–2 MB). The current screen's numbers include WebView2's
GPU process's private working set but not its GPU allocations, which were not measured. A first build that left GPUI
Kit in its light theme measured 16 / 19 / 22 MB; the table shows the corrected dark build with code colours.
Processor and start time come from the first build. The dust animation alone explains part of the current screen's
idle cost; the spike has no animation.

## What works and what does not

- **Drawing.** Markdown bold, lists and fenced code blocks render, and the thousand-message list draws its first and
  last rows on demand (screenshots: idle, bottom, top, mid-stream). Code blocks get colours from GPUI Kit's `syntect`
  feature once its theme is set to dark.
- **Selection, copy, IME.** The markdown view is configured as selectable and the composer is GPUI Kit's `Textarea`,
  whose input handler supports IME, but none of the three was exercised: a hidden desktop takes no input, and copying
  would touch the owner's real clipboard.
- **Accessibility: fails.** The UI Automation tree of the window, walked from a process on the same hidden desktop,
  holds the window controls and no message text in any Name, Value or Text field. Narrator reads that tree, so Narrator
  would read nothing of the conversation. The current screen exposes everything through WebView2.
- **Browser pane inside the window: works, with a catch.** The crates.io release has no webview; GPUI Kit's repository
  (commit `c8917c36`) has an experimental one over Wry and WebView2. Built from its example, a local page stays inside
  its region, and the window with its WebView2 processes took 85 MB plus 38 MB GPU. But WebView2 is a native child
  window: a GPUI popover drawn across the page is hidden where it overlaps (the same limit Faber's pane has today,
  `src-tauri/src/pane/native.rs`), and the example needs DirectComposition turned off.
- **Testing.** Faber's screen runner drives the screen through CDP and the DOM (99 scenarios, 93 check files). A GPUI
  screen has neither, and with no UI Automation tree it could only be driven by pixels and synthetic window messages,
  so the whole screen test suite would need a new driver before a single screen could move.

## Go / no-go against the current SolidJS screen

| Criterion | GPUI | Verdict |
|---|---|---|
| Memory | about 4× less with GPU memory counted, 6× without (31 + 18 vs 202 MB on this PC with 1,000 messages) | GPUI far ahead |
| Processor | 14% of a core while streaming, near zero idle | GPUI ahead (current streaming not measured) |
| Start | about a third of a second to the first frame | Good; no comparable number for the current screen |
| Accessibility | no text reaches UI Automation | **No-go** until GPUI exposes an accessibility tree |
| Browser pane | experimental, outside the published release; covers GPUI overlays | Possible, no better than today |
| Screen tests | no DOM, no UIA: the runner and 99 scenarios need a new driver | **No-go** for now |
| Rewrite cost | about 20,000 lines of screen code, all the motion and looks work, and the tests | Large |
| Library maturity | GPUI is pre-1.0 (`gpui-pre` 0.3.10); GPUI Kit 0.8.1 lacks features its repository has | Risky |
| macOS | GPUI's home platform (Metal); not measured here | Likely fine |

**Verdict: no-go for now; R459 (Electron, everything else Rust) stands.** The memory and processor win is real and
large, but a screen nobody can read with a screen reader and nobody can test with the current runner is not shippable,
and those two gaps belong to the framework, not to the spike. Worth measuring again when GPUI exposes an accessibility
tree (AccessKit or UI Automation); until then the gains are better pursued inside the current screen (the dust's 8%
idle processor and the 202 MB with a thousand messages on this PC are the first targets).

## Not covered

Cold start with a flushed file cache, macOS, a real IME session, selection and copy, a screen reader run by a person,
and the current screen's processor while streaming. The spike's `target/` (5.1 GB) and the GPUI Kit checkout were
deleted after the measurements.

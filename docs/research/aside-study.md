# Aside study: a browser and desk Faber agents can see and test with, cheaply

Dated 2026-11-11. A study and a plan: no Faber code changed. Behavior is described in our own words; nothing of Aside (code,
prompt text, names, icons) goes into Faber, whose product names carry no vendor word (AGENTS.md).

## Method and inventory

Static reading only; nothing was run, no account, no call to Aside's servers, nothing from its user-data folder. The
Windows installer (`AsideInstaller-1.0.1022.1-win-x64.exe`, 11 MB) is only a downloader: it carries an updater (a
rebranded Chromium updater) and fetches the browser from Aside's server at install time, so it proves nothing about the
agent. The study read the copy the owner had already installed (`C:\Program Files\Aside\Application\1.0.1022.1`, read-only).
Notes, carved strings and the extracted bundles are in `E:\Dev\re-corpus\aside-1.0.1022.1\` (outside the repository).

| Part | Where | What it is |
| :--- | :--- | :--- |
| Browser | `Aside.dll` (308 MB), `Aside.exe` | Chromium-based (version string 154) with private extension APIs; not disassembled |
| Agent manager | `AsideAgentManager/` | A browser extension: side panel, `background.js`, content scripts. Holds the `debugger`, `tabs`, `downloads` and native-messaging permissions and is the bridge to the daemon |
| Daemon | `AsideDaemon/win-x64/aside-daemon.exe` (133 MB) | A Node single-executable with the whole agent as plain JavaScript: model loop, REPL, browser layer, permissions, compaction, memory, CLI, MCP |
| Desk helper | `AsideDaemon/win-x64/computer-use/AsideComputerUse.exe` | C++/WinRT, installed as a packaged app. UI Automation, window capture, OCR, input |
| Skills | `AsideDaemon/win-x64/static/skills/builtin/` (61 files) | Agent instructions (computer use, vault, visual fallback, sites, documents) |
| Vault | `AsidePasswordManager/` | A second extension; secrets stay inside it |
| Memory engine | `native/aside-native.node`, an `aside-memory` binary | Rust and ONNX runtime, local; no model file ships (it downloads) |
| Sandbox | `sandbox/AsideWindowsSandboxHelper.exe` | Windows process sandbox for the agent's shell |

Correction to the starting notes: Aside does inject script. A 55 KB bundle runs in a hidden isolated world of every frame
(page scripts cannot see it) and a small console hook runs in the page's own world. Only the layer under them is thin.

## 3a. Reading the page

Observed: the agent calls a snapshot function. The in-page bundle walks the document (shadow roots, `aria-owns`, CSS
pseudo-text included) and renders an indented list: role, name, then marks for `ref`, level, hidden, scrollable, checked,
disabled, focused, selected, placeholder and, for canvases, size. Refs are `e12`, and `f1e1` inside a child frame. A ref
is kept while the element keeps its role and name, so the same control has the same ref next read; if a ref's element is
gone, the daemon finds a node with the same role, name and position among equals (budget 5,000 nodes). Options: `interactive`
(controls, scroll areas, landmarks, canvases only), `showHidden`, a `ref` or CSS `selector` to narrow, `maxDepth` (50).
Limits: a name is cut at 100 characters, a text node at 200 (160 kept), an accessible name at 300; the whole result is cut
at 16 KiB with a line saying how to narrow it. Password values print as a placeholder. The first read of a scope prints
the tree; later reads of the same scope print only a line diff. Controls found by pointer cursor, `onclick` or `tabindex`
get a hint mark. Iframes are read through their own session and merged; failed frames retry 3 times at 300 ms.

Evidence: the carved bundle `injected-aside-world.js` (modules `aria-snapshot/*` and `aside-world.ts`), and in the daemon
the snapshot module (`SNAPSHOT_TEXT_MAX_BYTES=16384`, `diffLines`, `takeSnapshot` through `Runtime.callFunctionOn`).

Compare. Claude's pane asks the engine for the full accessibility tree and prints `[uid] role "name" (value)`, folding below
depth 8 and cutting names at 200 (`index.chunk-CIsKxfLj.js:182220`); its tool can filter to controls, find by words (20
hits) and read plain text. Faber's `browse` returns visible text up to 24,000 characters on every call
(`engine-core/src/browser.rs` `MAX_TEXT`), with no ids, no diff, no controls list; targets are found by words (`host/src/pane/act.js`).
Best: Aside for tokens (controls-only first read, hard cap, diff afterwards); Claude's for fidelity (browser-native
tree, cross-origin frames free); Faber's is the largest per call and the least precise.

Adopt: a controls-first read with stable refs, state marks, per-name caps, a byte cap and a diff against the previous read of
the same tab; secret-looking values never printed (Aside also redacts card- and key-shaped text in stored pages: adopt
that for Faber's step log). Reject: its scan of every element's computed style (tens of ms on big pages); use the
accessibility tree where the engine gives it and walk the DOM only for what it misses.
Cost: first read 1 to 4 thousand tokens against up to 6 thousand now; a repeat read a few hundred; script about 20 KB;
refs map at most 1 MB per tab, cleared per read. These token figures are estimates until measured with `bench/`.

## 3b. The script interface

Observed: one persistent JavaScript scope (ES2023, no imports, one scope across calls, 120 s per call) with a Playwright-like
`page`, tab functions (`openTab`, `closeTab`, `listBrowserTabs`, `attachBrowserTab`), `snapshot`, an annotated shot, element
and page shots, `pdf`, `fetch` with the user's cookies, `fs` (session folder), `sleep`, and `console.log` as the only way
results come back (the last expression is not returned). Clicks and opens wait for the page to be interactive and stable;
before a click it checks the element is attached, visible, enabled, still, and the topmost thing at that point. Other
skills add globals (documents, Gmail, sites, the vault, the desk). The same REPL is on the command line and as an MCP server.

Evidence: `REPL_TOOL_DESCRIPTION` and `BROWSER_WORKFLOW_PROMPT` in the daemon; `waitForReady`, `checkHitTarget`,
`resolvePointerTarget` in the in-page bundle (a click on a label or custom checkbox is retargeted to its real control).

Compare. Claude's pane: one action per tool call plus `browser_batch` (a list run in order, stopping at the first error)
and a page-script tool. Faber: one action per call. Best: Aside for round trips (a whole login-and-check flow in one call);
Claude's batch for safety and ease; Faber's needs the most calls.

Adopt: the readiness checks before a click, label and custom-control retargeting, and one call that runs many steps.
Reject for now: a JavaScript REPL. It needs a script runtime and a sandbox (a utility process on demand, 30 to 50 MB, plus
an escape surface), and R125 asks for the lightest result. A step list run by the host gives most of the saving: up to 30
steps, 120 s, `wait_for`, `assert`, `extract` and stop-on-error, one result. Revisit a script only if a bench shows step
lists failing on real flows. Cost: no new process; tokens fall from one reply per step to one per flow.

## 3c. Events as messages

Observed: dialogs (alert, confirm, prompt) are accepted at once and the agent gets one `[system]` line with the text and
what was pressed; a download becomes a line with file name and saved path after an access check; a crashed page and file
choosers are events a script can wait on; a popup is queued with its opener and URL. Page console output is kept
by the in-page hook without running page code (no getters, no `toString`), after a site that detects developer tools
closed itself on bait objects. The daemon itself tracks network only to know when a page is idle.

Evidence: the page class events (`dialog`, `download`, `popup`, `crash`, `filechooser`) and the protocol events behind them;
the console interceptor script.

Compare. Claude's pane: the agent pulls (`read_console_messages`, `read_network_requests` with a body by id). Faber: only "the
person used the page" and the person's-turn marks. Best: Aside for not having to ask; Claude's for network detail.

Adopt: a "since your last call" block on every browse answer (at most 10 lines of 200 characters, each marked as the page's
words): dialogs handled, downloads, popups, crashes, console errors, failed or 4xx/5xx requests (method, host, path, status;
never headers, cookies or bodies). A `watch` action for the last N console and network lines on demand. Reject: auto-accept of
`beforeunload`-style prompts the person should see. Cost: a ring buffer of 200 entries per tab, under 1 MB; no tokens when quiet.

## 3d. The picture fallback

Observed: the agent is told to read the tree first, take a labeled shot (a PNG with each control's ref drawn over it) when
it needs to see, and use coordinates only for canvases, maps, sliders, drag handles and similar pixel work, going back to
refs right after. A short guide names the escalation order and says to switch strategy after 2 or 3 failed tries. The
coordinate set is click, double click, drag along a path, move, scroll, type, key press, shot of the visible area. When a
site blocks, the guidance is to look at the site's own requests and call them with the user's cookies (no dedicated tool).

Evidence: the visual fallback skill and `annotatedScreenshot` in the page class (`mouse`, `keyboard` use `Input.dispatch*`).

Compare. Claude's pane: coordinate clicks from a screenshot, with zoom. Faber: a small JPEG on every call goes to the
screen, and to the agent only for size checks. Best: Aside's labeled shot (the model points by the same refs as in the tree);
Faber's `sizes` check (does it fit, what the edge cuts) is something neither of the others does.

Adopt: shot on request only, labeled by ref, element shots with margin, zoom region; a pixel set with the rule "refs first".
Reject: telling the agent to replay private site calls: it bypasses the site's own login and consent surfaces and the
person's turn marks. Cost: one 0.3 to 0.5 MB image per request, never kept in memory (20 MB disk cap stays, R225).

## 3e. Agent tabs: background and fixed size

Observed: the agent's tabs open inactive in the person's own browser window and join a tab group, through the extension
(`chrome.tabs.create` with `active:false`); focus emulation makes a background tab behave as focused; a tab counts as stably
sized from 960x540. The fixed 1440x900 of the notes is not in the daemon or the extension (no device-metrics override): it is in
the browser itself, which was not disassembled. The daemon can also record a tab as video through ffmpeg.

Compare. Claude's pane: a view of its own, sized by device-metrics override with phone and tablet presets. Faber already has
an agent tab per conversation in its own pane, size presets and the fit check. Best: Faber and Claude (own surface, the
person's tabs untouched); Aside's trick matters because it shares the person's browser.

Adopt: keep the pane; add only focus emulation, so a covered or parked agent tab keeps its timers and animation frames.
Reject: video recording (disk and an encoder process).

## 3f. Permissions, approvals and secrets

Observed: three session modes (read only, guard, full access; guard is the default) plus per-action rules (allow, ask,
deny, approved) and file roots outside which reads and writes ask or deny; a shell sandbox on Windows. The browser asks per
action kind (read, modify, download) and the desk per app and per action, where taking the screen asks unless full access.
The vault is the only path to a password: the agent can list sites without secrets, ask the vault to fill a login, a card
or an identity into the page, make a password it can hold only as an opaque handle, and have the vault type a one-time
code; it is told never to read the code's QR or setup key. All tool output is wrapped as untrusted, with look-alike tags
neutralized. A skill for solving page challenges also exists.

Evidence: `hasPermission`, `sessionPermissionModeSchema`, the password skill, `neutralizePromptInjectionSyntax`.

Compare. Claude: per-app grants with tiers in the desk, site permissions in the pane. Faber: `browse` exists only in
full access (`offers_browser`), login, captcha and payment are the person's turn, blocked hosts, text marked as the site's
words (R161, R197). Best: Aside has the finer controls; Faber has the safest stance on secrets.

Adopt: modes for browse and desk that follow R140 (a card to allow one site or app for the session, read-only mode); masking of
password values; the untrusted wrapper with neutralized tags (Faber has it in text; keep it for new fields). Reject: a vault and
agent-side filling (Faber never reads or forwards credentials, AGENTS.md), and the challenge solver (captcha stays the
person's). Cost: negligible memory; one approval card flow.

## 3g. Memory and compaction

Observed: compaction starts at 85% of the model's window (user range 50 to 95%, a 64,000-token floor for big windows, a
95% ceiling) and replaces the history with one summary message plus lists of files read and changed. Memory is plain files
in the account folder (instructions, persona, facts about the user, lessons) read into the prompt, plus a local Rust engine
with an ONNX embedder for recall. The browser adds a diff, not a memory: unchanged reads cost nothing.

Compare. Faber uses each CLI's own compaction and `memory_search` over its own notes. Best: equal; nothing for Faber to
build. Adopt only the diff (3a). Reject: a second compaction or an embedder (R125: a resident model costs more than it earns).

## 3h. The desk on Windows (and Mac)

Observed (Windows). A packaged C++ app (`AsideComputerUse.exe`; sources `main`, `applications`, `observer`, `ocr`, `tray`,
`computer-use`, `computer-use-capture`). It lists apps and windows; reads a window as a UI Automation tree whose controls
get numbers (`[30] button Save`) that are valid only for the latest state; tracks change with Automation event handlers
(structure, property, focus); captures one window by handle with `PrintWindow`; reads text with the system OCR; and acts
with `SendInput`, `SetCursorPos` and a replaced system cursor after bringing the app to the front. The agent's guide says so
plainly: on Windows every action takes the screen, the user's mouse and keyboard lock, an on-screen glow marks it, and an
`endForeground` call gives it back; reads never take the screen. On Mac the guide says each task has its own cursor and queue
and the pointer stays put (background control through the Mac accessibility tree), with a foreground request only when
pixels need real input. Functions: get app (opens it, asks approval, prints state and guide), click by number or point,
set value, type text (up to 256 characters), paste, press key, scroll, select text, secondary action, drag, state with a
`query` filter, screenshot, list windows, quit, minimize, restore, set window frame. State after the first look is a diff;
values over 4,096 characters are cut; state waits up to 4 s for a loading indicator; if the user types, the next action
yields. Refusals the helper knows: protected, elevated or other-session target, stale or disabled element, locked screen,
no accessible content, unsupported action, screen control ended. Control is behind an experimental setting, with approvals
per app and action.

Evidence: helper imports (`PrintWindow`, `SendInput`, `SetCursorPos`, `SetSystemCursor`, `SetForegroundWindow`,
`SetWinEventHook`, `EnumWindows`, `Windows.Media.Ocr`, UI Automation handlers), its source paths and JSON keys, the computer-use
skill, `hasPermission` (`type: computer`).

Compare. Claude Desktop's desk (this session's own tools) is screenshot-driven with per-app grants and tiers (browsers read,
terminals click-only), also taking the screen and blind on Electron windows that are covered. Faber: nothing (`pc.*`
partial, M6). Best: Aside's number-addressed accessibility tree beats screenshots on tokens and precision; on isolation
nobody wins on Windows, which is the opening.

Adopt (Faber's own design, needing a prototype before it is promised): a Rust crate `desk` in the host, using UI Automation
from the process already running (no new resident process): read first (windows list, tree with numbers, diff, window-only
capture by `PrintWindow` or window capture, never the whole screen). Act through UI Automation patterns (invoke, value, toggle,
expand, select, scroll) and window messages for classic controls, which need no cursor, keyboard or focus. Only when a step
cannot be done that way, ask the person with a card that says it will take the screen for that step. Same refusals as above;
Chromium and Electron apps expose their tree only once an accessibility client asks, so the read starts that and says so when
it is incomplete. Reject: replacing the system cursor, locking input, OCR as a default, packaged-app registration. Cost:
tree walk 50 to 300 ms on a big app, 10 to 20 MB while reading, 0 idle; one capture at a time, 0.3 to 0.5 MB.

## Build order

Slices of 300 to 1,000 lines, each with a check that fails without it; code in `host/src/pane/`, `electron/pane.ts`,
`engine-core/src/browser.rs` and a new `desk/`. Measure tokens with `bench/` before and after each.

| # | Slice | Lines | Gain |
| :-- | :--- | :-- | :--- |
| 1 | Controls-first read with refs, state marks, caps, diff, masked secrets; click and type by ref | 600 | Reads 60 to 85% smaller on repeat; no wrong-target clicks |
| 2 | Since-last-call block (dialogs, downloads, popups, crashes, console, failed requests) and `watch` | 500 | Errors seen without asking or a picture |
| 3 | `steps`: a host-run step list with waits, asserts, extracts | 700 | One call per flow instead of one per click |
| 4 | Shots on request: labeled, element, zoom; focus emulation | 400 | Pixel work without a full screen each call |
| 5 | Desk read: window list, tree, diff, window capture, per-app approval | 900 | Any desktop window seen alone, for tokens of a tree |
| 6 | Desk act through patterns, then the take-the-screen card | 800 | Testing desktop apps without taking the mouse |
| 7 | Browse and desk modes (read only, ask, full) per R140 | 400 | Browse leaves full access only |

1 to 4 are Windows and macOS alike; 5 and 6 start on Windows. Slice 5 can run in parallel with 2 to 4 (different files).

## Decisions and rules touched

R161 and R197 (the agent's browser, words, turn rules: the refs read extends them; the turn marks stay), R195 and R225 (pane,
step log, 20 MB pictures), R121 and R140 (modes: slice 7 changes what ask modes offer), R125 (memory: no new resident process),
R459 (Electron plus Rust: where code lives), R557 (a decision row only for the owner's call). Map rows: "Page in Faber's
browser", `browse`, `pc.*` (`docs/product/areas.md`), parity M6 (`docs/product/parity.md`, now Phase 9). Owner's call, not
decided here: whether desk work (R11 puts PC apps fourth) moves ahead of other work, and whether browse should be offered
outside full access.

## Not confirmed without running the app

- The 1440x900 agent size and the background tab trick inside the browser build (needs the running app or Aside.dll study).
- Real token size of a snapshot on real pages, and the "70% smaller" claim: only caps and diffing were confirmed.
- Whether the Windows helper can act without the foreground; the guide and imports say it takes the screen, but a
  UI Automation pattern path was not seen. Same for how it captures (`PrintWindow` seen, window capture not).
- The daemon-to-helper transport and the full command list (key names seen, not the exchange).
- What the 40 MB of machine-learning libraries beside the helper do (the memory engine uses ONNX; no model ships).
- Mac background control (no Mac binary read); the approval card wording; how the Max plan gate is enforced.
- Any network-replay helper (none found in the REPL list); vault internals (outside this study).

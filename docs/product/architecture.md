# Architecture

How Faber is built and updated, what makes it better over time, and the curated catalog.

## Architecture

```text
App (Tauri 2: window, tray, notifications, auto-update)
 └─ UI (SolidJS + TypeScript, in the system webview) ── fixed command list ──┐
Host (Rust, `host/`): the command table every front end calls (window, command line, tests) ─┘
Engine (Rust crate inside the app today; a separate process later, so work survives the app)
 ├─ Lead agent: live CLI session + Faber toolbox (MCP)
 ├─ Drivers: Claude Code (stream-json), Codex (app-server)
 ├─ Capability manifest per CLI and version
 ├─ Router: quality floor + quota price + measured scoreboard
 ├─ Project memory: plain files in the project's .faber/ folder
 ├─ Snapshots and undo · policy and audit log
 ├─ Curated catalog and store
 └─ Live manifest: compatibility matrix, provider policy switch, driver descriptors
```

- **Rust, Tauri 2 and SolidJS.** Chosen on 2026-10-10, while the app was still small, for weight and memory: Tauri
  uses the system webview (WebView2 on Windows) instead of shipping Chromium, and the engine is native code with no
  Node runtime. SolidJS updates only what changed, which suits answers streaming in. C++ only where a library leaves
  no choice. The cost: three webviews instead of one Chromium (WebView2, WebKit on macOS, WebKitGTK on Linux), so
  anything heavy on screen (terminal, 3D viewer, video) is tested on all three. The first Electron version stays
  tagged as `electron-f1`.
- **Moving to Electron (R459, started 2026-10-29).** One Chromium on every system, with everything that is not
  Chromium, V8 or the screen in Rust. Every command already lives in one table, `host/` (`Host::dispatch`), which the
  Tauri shell relays to; the Electron one runs the same table as the `faber-sidecar` process (newline-delimited JSON on
  stdin and stdout), and what needs a window is a method of the host's `Shell` trait. The tray, the notifications,
  keeping the PC awake and starting with the system are decided in the host too: the sidecar sends them as `order`
  lines and the shell only carries them out. Only the browser pane's commands stay the shell's own; the plan and its phases are in
  `docs/superpowers/specs/2026-10-27-electron-rust-shell-design.md`.
- **A memory budget, enforced by a test.** Faber itself (window open, no CLI running) stays under 500 MB when it is
  complete; the end-to-end test fails above 400 MB (R125). The limit is a safety net: each change still aims at the
  lightest and most responsive result it can reach. Anything that keeps a
  process or a large DOM alive needs a reason and a measurement. Measured on 2026-10-10 as Task Manager counts it
  (private working set): 89 MB idle, 114 MB with a thousand-message conversation open (only the last 40 messages are
  drawn, more on scrolling up), 4 MB closed to the tray (the screen is freed; the engine keeps running and the window
  comes back where it was). For comparison, the Claude desktop app used 2.5 GB that day. A streaming reply reaches the
  screen up to 30 times a second, so an update carries only the steps that changed since the last one
  (`host/src/feed.rs`): sent whole, a long agent reply (thousands of steps, 11 MB) was parsed by the webview at each
  update, and on 2026-10-22 the window grew to 8.3 GB after a few hours of such replies. The engine keeps in memory
  only the conversation on screen and the ones something is still working on (`engine/src/engine/evict.rs`, R461):
  every conversation opened used to stay until Faber closed, tool outputs included, and the installed app reached 707 MB
  in the engine and 1.3 GB in all after three hours on 2026-10-27. The sidebar's list reads only the conversation files
  that changed since it last read them (`Store::list`), instead of decoding all of them (202 MB) twice a turn. The
  screen check `long session` opens twelve big conversations in turn and fails if the app keeps them.
- **CLIs that cost almost nothing while idle.** A Claude Code process takes about 120 MB just waiting (more in long
  sessions, and each session starts its own copies of the user's MCP servers), and it has no way to serve two
  conversations. So Faber starts it on the first keystroke, not when the window opens, and closes it after two idle
  minutes; the next turn resumes the same session. Codex's app-server can serve many conversations as threads, so one
  shared process per area holds them all (about 50 MB for three). Next: one shared copy of each MCP server for all
  sessions of a project. On Windows, automatic helpers and unattended conversation turns also queue while measured
  owned CLI private working sets reach half physical RAM or less than 2 GiB is available for the person. One new
  automatic start is admitted per fresh sample; the nonempty queue samples every two seconds, off the engine lock.
  Shared processes count once, including their root and owned children, and Stop cancels queued starts. Active work
  is never killed. This is admission headroom, not a hard cap on later process growth or native CLI subagent starts.
  Internal text-comparison judges retain their separate bounded concurrency and do not wait on this admission queue.
  The app measures the same way on macOS and Linux, from each owned process's resident bytes read between two checks of
  its creation time; resident bytes may count a shared page once per process, so the reading errs high. Other Unix
  systems keep the worker-count limits.
- **Languages.** English, Portuguese (Brazil), Spanish, French and German from the start, following the system
  language unless the user picks one in Settings. Every text is written by hand, never machine-translated; a new
  language needs a native speaker to review it.
- **The user's own setup comes along.** Projects use the skills, MCP servers and plugins already configured in each
  CLI. Faber's checked catalog store stays private: its resources reach a session through the existing launch
  inputs, without changing the person's CLI setup or installing into personal command/agent folders.
  The user's hooks are off by default (they can rewrite answers or block steps) and can be turned on in Settings.
- **Use everything each CLI has.** Each driver declares, per CLI version, what exists: resume, fork, MCP, skills,
  plugins, hooks, subagents, plan mode, sandbox, web search, image generation, computer use, review, quota readout.
  A probe checks this on install and on every update, reading what the CLI reports about itself, so new CLI features
  show up by themselves. If Faber cannot parse a CLI's output, it falls back to the raw terminal in one click instead of
  freezing.
- **Project memory is plain files.** A project's decisions, taste, plans and recipes sit in its `.faber/` folder, in
  open formats, with a README that explains them to any future AI. Each deliverable keeps a re-runnable recipe and
  carries its request and taste as provenance. The conversation history stays private inside Faber.

## Updates

- **Faber itself** updates itself. Today it downloads a signed update in the background and installs it on quit or, on
  Windows, at the first moment no conversation or CLI install runs and the window is hidden (R118). Installing
  when Faber opens (R141) is not built yet.
- **The CLIs.** Claude Code and Codex move only to versions that GitHub tested overnight with recorded conversations.
  If a version breaks, Faber stays on the previous one until a fix arrives as a published file, with no new Faber
  download.
- **Catalog tools** never update on their own. Faber suggests an update at a good moment, which is when nothing is
  running or when the update fixes something the user is using, and never in the middle of work. If the user agrees,
  Faber runs the update and the check. Before updating, it shows what changed, checks safety and keeps the old version.

## What makes it better

- **Measured quality.** The proof is defined before the work. A ghost client from another vendor uses the result like
  a regular person and records the screen. Reviewers are checked with a planted defect on a copy, adopted only after
  offline validation. When a new model ships, Faber replays the user's best past work and reports what improved.
- **Quota as a perishable resource.** For providers whose limits reset, Faber spends what would be lost near the
  reset on deferrable work. One to three CLIs stay warm within a visible RAM budget. The CLI starts booting at the
  first keystroke. Each step loads only the tools it needs, and related requests stay in the same session while the
  prompt cache is warm.
- **Safety for non-programmers.** Work in external apps happens on a copy the user promotes with one click. Canary
  tokens catch malicious catalog items. A stateful policy blocks any command it does not understand.
- **Learned taste.** Early on, Faber shows more options and learns preferences; later the questions shrink.

## Curated catalog and live manifest

The Catalog browsing tab is built (R344), separate from the two work areas. It manages 114 pinned skills, one tool server and checked plugin delivery; no plugin candidate passes the current audit, so none ships and the Plugins filter has no placeholder cards. Session kits choose installed skills and servers per task; first-run packs and the broader catalog/live-manifest proposals below remain later work. It unmounts on close, never polls or starts a CLI to browse, and pages the DOM at 48 cards and 48 inventory rows per CLI. Off and removed copies stay outside CLI roots; enabling checks disabled files, and local Undo journals the move and recovers it before pruning. The latest removal is retained only until the next Catalog visit or engine restart. Filesystem commands use blocking workers; inventory has a two-second deadline and one outstanding reader. Personal/project discovery reads bounded known skill folders and installation manifests only, showing unsupported overrides instead of guessing live CLI state.

A plugin groups checked skills, slash-command prompts, subagent definitions and R253-portable npm servers under
one catalog id. Commit and whole-folder digest pins cover the original source and every part; server packages retain
their pinned version and digest. The existing store lock, ledger, selection, journal and Undo govern the parent as one
item, including recovery and launch-time checks. Faber runs no shipped plugin scripts. Hooks, binaries, postinstall,
dependencies, unsupported delivery and unchecked parts keep the whole candidate out. The twenty original candidates
remain excluded with individual evidence in [the audit](../research/2026-10-21/catalog-list.md).

Claude Projects receives commands and agents alongside skills in its existing private session plugin when supported.
Codex uses its existing skill delivery and per-session agent configuration pointing to checked role TOML files;
custom slash-command prompts cannot be delivered within this boundary, so cards and details disclose their absence.
No personal CLI command/agent folder or configuration is written. Chat takes only selected skill instructions, including
plugin skills, and receives no plugin commands or agents. Servers follow the existing delivery and Full access gate;
plugin resources reuse the kit selector. Native definitions add only task instructions, never a permission or model override.
A Codex role is a Faber-written file holding only instructions (Codex documents no inline role instructions) and is compared
byte for byte at launch; Codex reads it again when a subagent spawns, so a rewrite after the thread started by something
with write access to the shared kit folder is the same integrity limit as for skills Codex reads lazily.
The implementation is `engine/src/tools/plugins.rs` and `engine/src/tools/plugin_delivery.rs`; local-fixture lifecycle
and delivery checks are `engine/tests/plugins.rs` and `tests/ui/plugins.check.ts`. No extra process or idle work is added;
native full-app memory and processor measurements remain the lead's validation.

- **Faber recommends only tools it has vetted.** A vetted tool works, is safe, has a license that allows the use, is
  maintained and improves the result. The agent may find and install any other tool. That tool is marked "not vetted",
  and the agent asks before installing it.
- Nothing ships in the installer. On first run the user picks the areas they work in; Faber shows the packs for
  those areas with the recommended items already ticked. Everything is optional: untick anything, or press "Install
  all recommended". Installs run in the background while the user starts working. Each item downloads from its
  original source with a pinned version, a checksum and its SPDX license on screen. Non-commercial licenses get a
  warning. Models that require accepting a license on their hosting site ask the user to accept it there.
- **Curation is strict.** An item enters only if it: has a license that allows the use; is maintained; installs
  from source in an automated smoke test; passes the security scan; is not a weaker duplicate of another item; and
  works with at least one supported CLI. Each item has a tier (Essential, Recommended, Extra, Experimental), the areas
  it serves, the CLIs it works with and its requirements (OS, GPU memory, apps such as Blender). "Recommended" needs
  evidence: the benchmark shows it helps, or it is the reference tool for its job. The first draft of the list has 1
  Recommended item, 123 Experimental items and 28 removed, and it keeps growing and getting curated over time.
- **Video and music are covered.** Composed video (HyperFrames plus FFmpeg) works with any subscription, because the
  CLI writes the composition. AI-generated music, voice and footage come from local models in the catalog when the
  user has a capable GPU.
- **Total freedom.** "Add your own" takes a GitHub link, a folder, a package, an MCP server command or URL, a plugin
  marketplace, or a skill the agent writes from a conversation. Users can also point Faber at other catalogs (a team's
  or a friend's). Own items go through the same security scan, which warns but never blocks.
- Updates are never automatic: diff, quarantine, previous version kept.
- The catalog is maintained by a weekly scout run, community pull requests and CI checks (license, link health,
  activity, smoke test, scan).
- Proposal, not built (R148): the same weekly scout also looks for new practices on coherence, review and harness
  quality, judges each against the project's rules, and asks the owner a yes or no before any becomes a rule. It then
  replaces the owner's own weekly scout that does this for Faber today.
- A public repository publishes signed static files (no backend): the catalog, a nightly compatibility matrix built
  from recorded sessions against the newest CLI versions (users only move to green versions), a per-provider policy
  switch, and declarative driver descriptors so a CLI format change can be fixed without an app release. If GitHub is
  down, Faber keeps the last good manifest.

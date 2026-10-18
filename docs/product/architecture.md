# Architecture

How Faber is built and updated, what makes it better over time, and the curated catalog.

## Architecture

```text
App (Tauri 2: window, tray, notifications, auto-update)
 └─ UI (SolidJS + TypeScript, in the system webview) ── fixed command list ──┐
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
- **A memory budget, enforced by a test.** Faber itself (window open, no CLI running) stays under 500 MB when it is
  complete; the end-to-end test fails above 400 MB (R125). The limit is a safety net: each change still aims at the
  lightest and most responsive result it can reach. Anything that keeps a
  process or a large DOM alive needs a reason and a measurement. Measured on 2026-10-10 as Task Manager counts it
  (private working set): 89 MB idle, 114 MB with a thousand-message conversation open (only the last 40 messages are
  drawn, more on scrolling up), 4 MB closed to the tray (the screen is freed; the engine keeps running and the window
  comes back where it was). For comparison, the Claude desktop app used 2.5 GB that day.
- **CLIs that cost almost nothing while idle.** A Claude Code process takes about 120 MB just waiting (more in long
  sessions, and each session starts its own copies of the user's MCP servers), and it has no way to serve two
  conversations. So Faber starts it on the first keystroke, not when the window opens, and closes it after two idle
  minutes; the next turn resumes the same session. Codex's app-server can serve many conversations as threads, so one
  shared process per area holds them all (about 50 MB for three). Next: one shared copy of each MCP server for all
  sessions of a project, and a RAM budget across all CLIs once the lead agent runs workers in parallel (workers queue
  instead of going over it).
- **Languages.** English, Portuguese (Brazil), Spanish, French and German from the start, following the system
  language unless the user picks one in Settings. Every text is written by hand, never machine-translated; a new
  language needs a native speaker to review it.
- **The user's own setup comes along.** Projects use the skills, MCP servers and plugins already configured in each
  CLI, and Faber's catalog installs into that same configuration, so a tool works the same inside and outside Faber.
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

The catalog screens and the catalog step in the first run arrive in Phase 3, with the lead agent's toolbox.

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

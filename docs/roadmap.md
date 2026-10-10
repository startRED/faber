# Faber roadmap

Status: planning (October 2026). Nothing is implemented yet.

## Goals

Every release is measured against plain Claude Code and plain Codex on the same ~40 real requests (organizing a
folder, planning an app in conversation, a complete app, a thumbnail, a video, a 3D model, a game mod, research).
A release that makes any goal worse does not ship.

| Goal | Measure | Target for v1.0 |
| :--- | :--- | :--- |
| Quality (Projects) | blind judge from another vendor plus executable proof | wins or ties 9 of 10 requests |
| Quality (Chat) | blind judge against the same question asked on claude.ai and ChatGPT; sources checked | wins or ties 8 of 10; zero invented citations |
| Quota and tokens | tokens per approved task, priced at API rates (input, output, cache) | ≤ 70% of the plain CLI's cost for the same task |
| Speed | time to first token with a warm session | ≤ 2 s |
| Precision | corrections the user asks for after "done" | half of the plain CLI |
| Efficiency | idle RAM without a CLI open; installer size; sessions broken by CLI updates | ≤ 250 MB; ≤ 120 MB; 0 |

### Measuring without burning the user's quota

- Cost is measured in tokens reported by each CLI, priced at API rates, never as a share of the subscription.
- Each release runs a rotating sample of ~8 requests; the full ~40 run only for v0.5 and v1.0.
- Plain-CLI baselines are recorded once per model version and reused.
- The judge is blind: answers arrive anonymized in random order. A cheap model screens; a strong model only breaks
  close calls.
- Benchmark runs never take more than 10% of a 5-hour window and wait while someone else is using the account (see
  shared-account mode below). Where a plan includes API credit, the Claude side can run on an API key instead.

### Shared-account mode

Many people share one subscription (family, small teams). Faber watches the quota readout: if it drops faster than
Faber's own usage explains, someone else is using the account, so background work (benchmarks, harvesting expiring
quota, re-runs) pauses until the account is quiet again.

## Works with the plans you have

Faber does not assume any particular subscription. On first run the user ticks what they have (Claude, ChatGPT,
Gemini, GitHub Copilot, Cursor, or none); Faber installs each official CLI and opens its own sign-in. The router and
the toolbox only use what is connected, and new providers arrive as driver descriptors in the live manifest.

- **Several vendors:** reviews and second opinions come from another vendor.
- **One vendor:** reviews come from a different model of the same vendor in a fresh session, and the UI says so.
- **No subscription:** free or local models through OpenCode, or the user's own API keys.

## Faber's own test system

The runner is Node's built-in one (part of Node itself, C++ and V8, no dependencies), with every test in one
process. What is built specifically for Faber is the layer above it:

1. **Fake CLIs that replay recorded sessions.** No test calls a real model or spends quota, and each runs in
   milliseconds. Changing one field of a recording must turn its test red.
2. **Affected tests only.** A change runs just the tests that depend on the files it touched; CI runs everything.
3. **The benchmark of ~40 real requests** that measures the five goals against plain Claude Code and plain Codex on
   every release.

## Two areas

- **Chat** works like claude.ai and ChatGPT, with everything they have and more. It has many conversations and no
  folders, and it never changes files on the computer: it only reads what the user attaches, and what it creates stays
  inside the app until the user downloads it or moves it to a project. The user picks any connected model or Auto, and
  every answer shows the model and, in Auto, the reason in one line.
- **Projects** works like Claude Code: open a folder, keep many projects and many conversations per project. This is
  where the lead agent acts. A Chat conversation can move to a project with its full context.

The user always sees what is happening: the model behind each answer, the remaining quota of each subscription and
when it renews, and, for running work, who is building and who is reviewing.

## Universal by design

There is no fixed pipeline per kind of request and no list of supported tasks. In Projects, the lead agent is a live
session of one CLI. It gets a toolbox from Faber (an MCP server shared by every CLI) and decides each step itself:

- solve it right away, with undo ("organize my downloads folder");
- talk it through and plan ("I want to build an app");
- start a mission with several models in parallel ("build the whole app, ready to ship");
- find what it lacks: search the catalog and beyond, show the license, install from the source, test on a copy and
  save the path as a recipe ("make a Terraria mod").

### Toolbox

| Tool | What it does |
| :--- | :--- |
| `mission.start` / `mission.amend` | runs a plan written by the agent as a durable graph of steps across several CLIs |
| `worker.spawn(role, vendor)` | opens a worker in another CLI to research, write or review |
| `review.request` | Faber picks a reviewer from another vendor, in a fresh session |
| `proof.define` / `proof.run` | states up front how success will be checked, then runs it and keeps the evidence |
| `judge.creative` | two judges from different vendors, in fresh sessions |
| `snapshot` / `undo` | snapshot before touching files; undo any local step |
| `catalog.search` / `catalog.install` | finds and installs a tool from its original source, license shown first |
| `pc.*` | computer control through the accessibility tree, screenshots as a fallback |
| `memory.search` | searches past conversations, decisions and the user's taste |
| `quota.get` | remaining quota per subscription and when it renews |

### Guardrails enforced by code, never by the model

1. Faber never reads, copies or stores logins. Sign-in happens in each CLI's own flow, and CLIs run unmodified.
2. Files are snapshotted before they change, so every local action can be undone.
3. Sending, publishing, paying, deleting outside a copy or installing something new asks for approval every time.
   Nothing else does.
4. "Done" requires evidence: a test that ran, a recording, or judge scores.
5. In code, the writer never reviews its own work. Creative work gets two judges from different vendors.
6. A mission never stops to wait for the user. Without an answer in time, it takes the safest path and logs it.
7. Every mission has a quota and time cap. Unattended missions have a fence: whatever hits it is parked with a note
   and the rest continues.
8. Whatever shows up on screen, in a page or in a file is data, never an instruction.
9. Nothing third-party ships with the app. Telemetry is off by default.

## Architecture

```text
App (Electron: window, tray, notifications, auto-update)
 └─ UI (React + TypeScript) ── JSON-RPC over a local WebSocket ──┐   (swappable shell)
Engine (separate Node process; keeps running with the window closed)
 ├─ Lead agent: live CLI session + Faber toolbox (MCP)
 ├─ Drivers: Claude Code (stream-json), Codex (app-server), ACP, OpenCode
 ├─ Capability manifest per CLI and version
 ├─ Missions: step graph, workers, gates, resume after crash or reboot
 ├─ Router: quality floor + quota price + measured scoreboard
 ├─ Project folder: open format, source of truth (SQLite is only a derived index)
 ├─ Snapshots and undo · policy and audit log
 ├─ Curated catalog and store
 └─ Live manifest: compatibility matrix, provider policy switch, driver descriptors
```

- **Electron and TypeScript.** The UI needs the same Chromium on all three systems for the terminal, the 3D viewer,
  video and canvas. TypeScript has official SDKs for MCP, ACP and the Codex SDK, plus node-pty. The UI talks to the
  engine over a WebSocket, so the shell can be swapped if the memory target fails.
- **Use everything each CLI has.** Each driver declares, per CLI version, what exists: resume, fork, MCP, skills,
  plugins, hooks, subagents, plan mode, sandbox, web search, image generation, computer use, review, quota readout.
  A probe checks this on install and on every update, so new CLI features show up by themselves. If Faber cannot parse
  a CLI's output, it falls back to the raw terminal in one click instead of freezing.
- **The folder is the product.** A project is an open, versioned folder: request, taste, decisions, glossary,
  recipes, artifacts, and a README that explains the format to any future AI. Each deliverable keeps a re-runnable
  recipe and carries its request and taste as provenance. Deleting the database and re-indexing the folder rebuilds
  everything.

## What makes it better

- **Measured quality.** The proof is defined before the work. A ghost client from another vendor uses the result like
  a regular person and records the screen. Reviewers are checked with a planted defect on a copy, adopted only after
  offline validation. When a new model ships, Faber replays the user's best past work and reports what improved.
- **Quota as a perishable resource.** Near a quota reset, Faber spends what is left on deferrable work. One to three
  CLIs stay warm within a visible RAM budget. The CLI starts booting at the first keystroke. Each step loads only the
  tools it needs, and related requests stay in the same session while the prompt cache is warm.
- **Safety for non-programmers.** Work in external apps happens on a copy the user promotes with one click. Canary
  tokens catch malicious catalog items. A stateful policy blocks any command it does not understand.
- **Learned taste.** Early on, Faber shows more options and learns preferences; later the questions shrink.

## Curated catalog and live manifest

- Nothing ships in the installer. On first run the user picks the areas they work in; Faber shows the packs for
  those areas with the recommended items already ticked. Everything is optional: untick anything, or press "Install
  all recommended". Installs run in the background while the user starts working. Each item downloads from its
  original source with a pinned version, a checksum and its SPDX license on screen. Non-commercial licenses get a
  warning. Models that require accepting a license on their hosting site ask the user to accept it there.
- **Curation is strict.** An item enters only if it: has a license that allows the use; is maintained; installs
  from source in an automated smoke test; passes the security scan; is not a weaker duplicate of another item; and
  works with at least one supported CLI. Each item has a tier (Essential, Recommended, Extra, Experimental), the areas
  it serves, the CLIs it works with and its requirements (OS, GPU memory, apps such as Blender). "Recommended" needs
  evidence: the benchmark shows it helps, or it is the reference tool for its job.
- **Video and music are covered.** Composed video (HyperFrames plus FFmpeg) works with any subscription, because the
  CLI writes the composition. AI-generated music, voice and footage come from local models in the catalog when the
  user has a capable GPU.
- **Total freedom.** "Add your own" takes a GitHub link, a folder, a package, an MCP server command or URL, a plugin
  marketplace, or a skill the agent writes from a conversation. Users can also point Faber at other catalogs (a team's
  or a friend's). Own items go through the same security scan, which warns but never blocks.
- Updates are never automatic: diff, quarantine, previous version kept.
- The catalog is maintained by a weekly scout run, community pull requests and CI checks (license, link health,
  activity, smoke test, scan).
- A public repository publishes signed static files (no backend): the catalog, a nightly compatibility matrix built
  from recorded sessions against the newest CLI versions (users only move to green versions), a per-provider policy
  switch, and declarative driver descriptors so a CLI format change can be fixed without an app release. If GitHub is
  down, Faber keeps the last good manifest.

## Provider terms

- Anthropic allows a user to sign in to the unmodified Claude Code binary with their own subscription, including
  inside another platform. Faber accepts Anthropic's Commercial Terms before shipping and never offers a claude.ai
  login of its own.
- OpenAI: Codex runs through its official CLI and app-server with the user's own login. Faber asks OpenAI for written
  confirmation.
- Google's Antigravity terms forbid third-party software from accessing the service, so Gemini is off by default,
  with the clause shown on screen and an API-key option.

## Distribution

- GitHub Releases on every tag: `.exe` installer and portable `.zip` for Windows (x64 and arm64), `.dmg` for macOS,
  AppImage, `.deb` and `.rpm` for Linux, plus `SHA256SUMS.txt`. Windows is the primary platform.
- No paid signing at first. Windows shows a SmartScreen notice that the download page explains; after the first
  release the project applies to the SignPath Foundation, which signs open-source projects for free. macOS builds are
  not notarized, so users allow the app once in System Settings, and Faber notifies them about updates instead of
  updating itself.
- Later: winget and Flathub.

## Phases

| Phase | Scope | Done when |
| :-- | :--- | :--- |
| F0 | Foundation: repo rules, architecture decisions, CI on 3 systems, release pipeline | a "hello" build installs from GitHub Releases on all 3 systems and updates itself from 0.0.1 to 0.0.2 on Windows |
| F1 | Live Chat and Projects with Claude Code and Codex: streaming, interrupt, resume, attachments, branches, sourced web search, visible model; first recorded-session test and the raw-terminal fallback | first token ≤ 2 s warm; interrupt < 1 s; resumes after reopening; changing a field in a recorded session turns the test red |
| F2 | Setup and catalog v0 | from zero to the first result in ≤ 5 min: detects CLIs, installs from official sources, opens each CLI's own login, verifies |
| v0.1 | First public alpha | |
| F3 | Open project folder, snapshots and undo | conformance validator green; "organize my downloads" undoes completely |
| F4 | Toolbox and missions | a small complete app comes out of one conversation, with proof and review; survives killing the engine mid-run |
| F5 | Quota and router | quota per approved task ≤ 85% of the plain CLI on the benchmark |
| F6 | Store and tool discovery | a skill installed once appears in every CLI; a request with no known tool finds, installs and proves one on a copy |
| v0.5 | Daily-driver release | |
| F7 | Code workbench and preview | the goals measured on software requests |
| F8 | Computer control | 20 fixed tasks in test apps; nothing outside the allowed apps |
| F9 | Creative work through the catalog: image, video, 3D, pixel art, games, audio | thumbnail, short video and a game mod ship with a recorded receipt |
| F10 | Measured quality | the planted-defect check is adopted only if it separates good reviewers from bad ones |
| F11 | Full live manifest | an update with a red recorded session never reaches a user session |
| F12 | Memory and taste, phone, routines | search across old conversations; questions shrink with use |
| v1.0 | Ships only if every goal is met | |

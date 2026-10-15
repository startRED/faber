# Faber: guide for agents

Faber is a source-available desktop app (FSL-1.1-ALv2) with a Chat area and a Projects area that run the official AI CLIs
with the user's own subscriptions. Read [docs/roadmap.md](docs/roadmap.md) before changing scope.

## Rules

- Write code, comments, docs and commit messages in English.
- Original code only. Never copy code from TL-ADE, TL-Control or TL-Orchestrator; reusing their ideas is fine.
- Never read, copy, store or forward CLI credentials or session tokens (`~/.claude`, `~/.codex`, Gemini OAuth). Users
  sign in through each CLI's own flow, and the CLI binaries run unmodified.
- Never bundle third-party tools, skills, models or MCP servers in the app or the repo. They come from the curated
  catalog and are downloaded from their original source.
- Do not use vendor names (Claude, Anthropic, Codex, OpenAI, ChatGPT, Gemini, Google) in product, feature or package
  names. Plain-text statements such as "runs Claude Code" are fine.
- Child processes never go through a shell. On Windows, npm's `.cmd` shims cannot start without one, so the engine
  resolves the real executable (`engine/src/cli.rs`). Anything that does build a shell line (`src-tauri/src/terminal.rs`)
  checks every piece first.
- Every text on screen lives in `ui/locales/`. Each language is written by hand, never machine-translated; English
  (`en.ts`) is the source, and a new language needs a native speaker to review it. The engine returns keys
  (`error.*`, `reason.*`) instead of sentences.
- Memory is a feature: Faber with no CLI running stays under 500 MB when complete, and the end-to-end test fails above
  400 MB (R125). The limit is a safety net, not the goal: every change aims at the lightest and most responsive result
  it can reach and reports its memory and processor cost. Anything that keeps a process or a big DOM alive needs a
  reason and a measurement ([docs/roadmap.md](docs/roadmap.md#architecture)).
- Telemetry stays off by default.
- One commit per coherent change, made and pushed as soon as the change is proven, without waiting for other parts.
  Commits may group related steps, but never huge: at least one proven commit pushed to `phase-1-base` every hour of
  work, and proven work is never left unpushed. CI fast-forwards `main` to each green `phase-1-base` commit by itself
  (never force, never a red commit), so `main` moves at least hourly (R126, R127, R131).

## Stack

- `engine/`: the engine in Rust (tokio). It drives the CLIs, keeps conversations as JSON files and knows nothing
  about windows.
- `src-tauri/`: the Tauri 2 shell. One window, a tray icon, and the fixed list of commands the screen may call.
- `winproc/`: the one unsafe call (resuming a process created suspended, so it joins its job before it runs). Everything else forbids unsafe code.
- `ui/`: the screen in SolidJS + TypeScript, built by Vite into `dist/`.
- `tests/`: the screen's checks (`tests/ui/*.check.ts`, run inside a hidden test build) and the recordings the fake CLI
  replays (`tests/fixtures/`). The engine's own tests replay the same fixtures (`engine/tests/`).
- `testkit/`: Faber's own test runner (`cargo t`) and the fake CLI. It runs only the tests that depend on what changed,
  launches a hidden test build inside a sandbox and audits every run ([docs/design.md](docs/design.md#screen-tests)).

## Proof commands

All exit 0 before any commit:

```bash
npm run lint
npm run typecheck
npm run build
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p faber --features test-hooks -- -D warnings
cargo t
```

`cargo t` runs only the tests that depend on the files you changed; `cargo t --all` runs everything, as CI does (the
`rust` and `ui` suites everywhere, `screen` on Windows). Run `--all` when the screen or the shell changes. `npm run dev`
opens the app from source with live reload. Unit tests never spawn processes, touch real git or call a real CLI.

On the maintainer's PC, Rust lives outside the default folders; export these before any `cargo` or `npm run dev`:

```bash
export RUSTUP_HOME='E:\Apps\rust\rustup' CARGO_HOME='E:\Apps\rust\cargo' PATH="/e/Apps/rust/cargo/bin:$PATH"
```

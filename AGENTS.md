# Faber: guide for agents

Faber is an open-source desktop app (Apache-2.0) with a Chat area and a Projects area that run the official AI CLIs
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
- Telemetry stays off by default.
- One commit per coherent change, made as soon as the change is proven.

## Stack

- `engine/`: the engine in Rust (tokio). It drives the CLIs, keeps conversations as JSON files and knows nothing
  about windows.
- `src-tauri/`: the Tauri 2 shell. One window, a tray icon, and the fixed list of commands the screen may call.
- `ui/`: the screen in SolidJS + TypeScript, built by Vite into `dist/`.
- `tests/`: UI tests (`*.test.ts`, Node's runner) and the end-to-end test (`tests/e2e/`), which drives the real app
  through WebView2's DevTools port with a fake CLI replaying `tests/fixtures/`. The engine's own tests replay the same
  fixtures (`engine/tests/`).

## Proof commands

All exit 0 before any commit:

```bash
npm run lint
npm run typecheck
npm test
npm run build
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

`npm run test:e2e` (Windows) builds the app and runs the end-to-end test in a hidden window; run it when the screen
or the shell changes. `npm run dev` opens the app from source with live reload. Unit tests never spawn processes,
touch real git or call a real CLI.

On the maintainer's PC, Rust lives outside the default folders; export these before any `cargo` or `npm run dev`:

```bash
export RUSTUP_HOME='E:\Apps\rust\rustup' CARGO_HOME='E:\Apps\rust\cargo' PATH="/e/Apps/rust/cargo/bin:$PATH"
```

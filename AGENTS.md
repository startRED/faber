# Faber: guide for agents

Faber is a source-available desktop app (FSL-1.1-ALv2) with a Chat area and a Projects area that run the official AI CLIs
with the user's own subscriptions. [docs/README.md](docs/README.md) lists the living pages: read what your task needs, never
a folder; `docs/history/` and dated specs are never a source. Before changing scope read [docs/product/](docs/product/README.md);
before changing what a person sees, sets or calls follow [docs/product-rules.md](docs/product-rules.md) and search
[docs/product-map.md](docs/product-map.md) and [docs/decisions.md](docs/decisions.md) for its words, never reading them whole.

## Rules

- Code, comments, docs and commit messages in English. Original code only: never copy TL-ADE, TL-Control or TL-Orchestrator.
- Never read, copy, store or forward CLI credentials or session tokens (`~/.claude`, `~/.codex`, Gemini OAuth). Users sign
  in through each CLI's own flow, and the CLI binaries run unmodified.
- Never bundle third-party tools, skills, models or MCP servers: the curated catalog downloads them from their source.
- No vendor names (Claude, Anthropic, Codex, OpenAI, ChatGPT, Gemini, Google) in product, feature or package names.
- Child processes never go through a shell (`engine-core/src/cli.rs` resolves npm's real executable); code that builds a shell
  line (`host/src/terminal.rs`) checks every piece first. Unit tests never spawn processes, touch real git or call a real CLI.
- Every text on screen lives in `ui/locales/`, each language written by hand, never machine-translated; English (`en.ts`)
  is the source and a native speaker reviews each new language. The engine returns keys (`error.*`, `reason.*`).
- What's-new entries are written once a week by one session, never per change ([how](docs/dev/whats-new.md), R557). A decision
  row is only for a direction the owner chose or a rule that changes what a person gets; one that goes against a row goes to him.
- Memory is a feature: with no CLI running Faber stays under 500 MB and the end-to-end test fails above 400 MB (R125). Aim
  for the lightest result; a change to processes, long-lived state or how much the screen keeps drawn states its memory and
  processor cost in the commit body; a process or big DOM kept alive needs a reason and a measurement. Telemetry is off by default.
- At most three lanes at once, one per area ([lanes](docs/dev/workflow.md#lanes)); no manager of leads, no conversations
  sending each other work. Only the owner, or the session he names, adds tasks (at most 15); ask him only what is his
  (direction, taste, money) and go on meanwhile.
- A lane takes the biggest coherent slice (one feature or fix with its tests, often 300 to 1,000 lines), ships it, checks it
  in the app built from the base, then takes the next task (`node scripts/tasks.mjs next`). Never wait idle on a helper,
  another session or a run: hand out only pieces your next step does not need and read each result when its notice comes (R181).
- Review by risk: engine state, stored data, permissions, processes and security changes get a fresh session that did not
  write them, from the other vendor when available (else a new read-only session; two for security, R146); the rest ships on proofs.
- Done means a check fails without the change, and the product map and the design page of what changed follow in the same commit.
- One commit per coherent change, landed with `node scripts/ship.mjs` (R556): it squashes onto the newest `phase-1-base`,
  runs the quick proofs and pushes. CI checks after; `main` moves only to a green commit; a red one is reverted for its
  author to ship again ([landing](docs/dev/landing.md#fast-lane)). Never push to `phase-1-base` by hand or force; a red
  base comes first ([red base](docs/dev/workflow.md#red-base)). A `feat`, `fix` or `perf` commit that touches what ships
  becomes a patch release once green (R130); `docs`, `test`, `ci`, `refactor` and `chore` do not.

Stack: `engine/` (Rust: the `Engine`) on `engine-drivers/` (the CLI drivers) on `engine-core/` (JSON files, no windows);
`host/` (the one command table every front end calls, R459) with `sidecar/`, `channel/`, `cli/`; `electron/` (the shell;
`src-tauri/` ships until the release switch); `winproc/` (unsafe process calls); `ui/` (SolidJS + TypeScript); `tests/`
(screen checks); `testkit/` (`cargo t`). [Memory and processes](docs/product/architecture.md#architecture).

## Proof commands

All exit 0 before shipping (`ship.mjs` runs the same quick proofs on the exact tree); inside Faber use its `verify` tool.

```bash
npm run lint && npm run typecheck && npm run docs && npm run build && cargo fmt --all --check && npm run test:scripts && node scripts/cargo.mjs t --claims
```

Heavy suites (`cargo t --all`, Clippy, macOS) run in CI on the landed commit, not on the PC ([testing](docs/dev/testing.md)). A
command over two minutes runs in the background: wait for its notice, never a `sleep` or `until` loop, never poll CI. Compile
through `node scripts/cargo.mjs <cargo arguments>`. On the maintainer's PC, Rust lives outside the default folders; export
before any `cargo` or `npm run dev`:

```bash
export RUSTUP_HOME='E:\Apps\rust\rustup' CARGO_HOME='E:\Apps\rust\cargo' PATH="/e/Apps/zig:/e/Apps/rust/cargo/bin:$PATH"
```

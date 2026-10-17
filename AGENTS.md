# Faber: guide for agents

Faber is a source-available desktop app (FSL-1.1-ALv2) with a Chat area and a Projects area that run the official AI CLIs
with the user's own subscriptions. Read [docs/roadmap.md](docs/roadmap.md) before changing scope, and
[docs/product-map.md](docs/product-map.md) before changing anything a person sees, sets or calls.
[docs/README.md](docs/README.md) says which page answers what; dated specs, plans and research are history, never a source
for how Faber works today. This file stays short: each line prevents a real mistake, and a rule a test enforces leaves it.

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
- Every change a person would notice adds an entry (`notes.next.<new|better|fixed>`) to every `ui/locales/*.ts` file
  in the same pull request: one short sentence in plain words, written by hand. The id only grows, never depends on the
  version, and is given at merge time with the decision numbers ([design.md](docs/design.md#numbers)). After an update the screen shows what is newer than the last entry seen, and the
  release body is built from the English entries (R142, `scripts/release-notes.mjs`).
- Memory is a feature: Faber with no CLI running stays under 500 MB when complete, and the end-to-end test fails above
  400 MB (R125). The limit is a safety net, not the goal: every change aims at the lightest and most responsive result
  it can reach and reports its memory and processor cost. Anything that keeps a process or a big DOM alive needs a
  reason and a measurement ([docs/roadmap.md](docs/roadmap.md#architecture)).
- Telemetry stays off by default.
- One commit per coherent change, made and pushed as soon as the change is proven, without waiting for other parts.
  Commits may group related steps, but never huge: push often, roughly once an hour is the usual pace (a rough
  average, not a target), and never leave proven work unpushed. CI fast-forwards `main` to each green `phase-1-base`
  commit by itself (never force, never a red commit), so `main` keeps close behind (R126, R127, R143). Lane work (a
  branch or worktree) enters `phase-1-base` as one squashed pull request that fills `.github/pull_request_template.md`
  and whose title is a commit title (`pr-title` checks it). A small proven commit goes through the queue instead of
  straight there: `git push origin HEAD:refs/heads/queue/<name>`; CI lands it on `phase-1-base` and `main` only when
  every system is green, and a red one waits on its queue branch for its author to fix (R162). Right before a merge,
  `node scripts/numbers.mjs assign` turns `R-next` and `notes.next` into real numbers.
- A red `phase-1-base` comes first: fix it, or keep your commits until it is green. `cargo t` shows it (`BASE` lines)
  and its `pre-push` hook refuses a push onto it ([design.md](docs/design.md#red-base)); `FABER_PUSH_ON_RED=1` pushes
  the fix.
- In a worktree on a branch, claim the areas and files you will change in `.faber-lane` before changing one
  ([design.md](docs/design.md#lane-claims)); `cargo t` refuses unclaimed changes and files another lane claims.

## Before a change a person will see or use

Read [docs/product-rules.md](docs/product-rules.md) and [docs/product-map.md](docs/product-map.md) first, then:

- Look in the product map for what already exists, in either area: the same setting, screen, control, words, layout,
  stored value or shell command. Extend it; a second way to do the same thing is a bug. Check its "Known incoherences"
  before touching those parts.
- Place it with the table in product-rules.md. A setting never lands on a work screen, and a new first-run step needs a
  reason no default covers.
- Check the taste cards whose "Touches" match. The review says "cards checked: C00x ok" or names the line that breaks one.
- Walk it as a newcomer and as a heavy user: first run, no AI connected, empty and waiting states, a 720 px window,
  reduced motion, the longest language.
- Something that goes against a decision ([docs/decisions.md](docs/decisions.md)) or a card goes to the owner first.
- How it looks and moves: [docs/design.md](docs/design.md) and the checklist in `.claude/skills/faber-ui/SKILL.md`
  (Claude loads it as a skill; other agents read the file).

## Done means

- The proof commands below pass, and a check fails without the change (the product map names the check per feature).
- The same commit updates [docs/product-map.md](docs/product-map.md) when a setting, screen, stored value, shell command,
  feature or product word appears, changes or goes (`engine/tests/product_map.rs` fails when one is missing), deletes the
  "Known incoherences" lines it fixed, and updates [docs/design.md](docs/design.md) when how a screen looks or behaves
  changed. A new decision is written as `R-next` in [docs/decisions.md](docs/decisions.md) and numbered at merge; a finished spec or plan gets
  its status in [docs/README.md](docs/README.md).
- The commit type is a release decision: a `feat`, `fix` or `perf` commit that touches what ships becomes a patch release
  by itself once CI is green (R130); `docs`, `test`, `ci`, `refactor` and `chore` do not.
- For a change a person sees, the commit body says `Extends: <map entry>`, or `New: <the closest existing thing and why
  it does not fit>`.
- The commit body states the memory and processor cost measured, or why there is none (docs, tests only).
- Code nobody can check by reading it once gets a review from a fresh session that did not write it: the other vendor
  when it is available, the same vendor in a new read-only session when its quota is poor (R146). Security changes get
  two. Every review follows the review rules of product-rules.md, and a change a person sees also gets its coherence
  review.

## Stack

- `engine/`: the engine in Rust (tokio). It drives the CLIs, keeps conversations as JSON files and knows nothing
  about windows.
- `src-tauri/`: the Tauri 2 shell. One window, a tray icon, and the fixed list of commands the screen may call.
- `winproc/`: the unsafe process calls (resuming a process created suspended, so it joins its job before it runs, and
  lowering a mission proof to Low integrity before it runs).
  The shell denies unsafe code except in its three Windows API files (`awake.rs`, `autostart.rs`, `tray/native.rs`);
  everything else forbids it.
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
cargo t --suite lint
cargo t
```

`cargo t --suite lint` runs both Clippy proofs (`--workspace --all-targets`, and `-p faber --features test-hooks`, each
with `-D warnings`), and on Windows a third for macOS through zig ([design.md](docs/design.md#what-a-run-prints)). `cargo t` runs only the tests that depend on the files you changed; `cargo t --all` runs everything,
as CI does (the `rust` and `ui` suites everywhere, `screen` on Windows). Run `--all` when the screen or the shell changes.
Both print only failures, measurements and a summary; every line is in `target/faber-test/run.log`. A command that may
take over two minutes (`cargo t`, a build) runs in the background while you keep working; wait for its notice or the
Monitor tool, never a `sleep` or `until` loop, and never poll CI (the app watches it). `npm run dev`
opens the app from source with live reload. Unit tests never spawn processes, touch real git or call a real CLI.

On the maintainer's PC, Rust lives outside the default folders; export these before any `cargo` or `npm run dev`:

```bash
export RUSTUP_HOME='E:\Apps\rust\rustup' CARGO_HOME='E:\Apps\rust\cargo' PATH="/e/Apps/zig:/e/Apps/rust/cargo/bin:$PATH"
```

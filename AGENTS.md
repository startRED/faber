# Faber: guide for agents

Faber is a source-available desktop app (FSL-1.1-ALv2) with a Chat area and a Projects area that run the official AI CLIs
with the user's own subscriptions. [docs/README.md](docs/README.md) lists every living page and when to read it: read the
pages your task needs, never a whole folder. Before changing scope read [docs/product/](docs/product/README.md); before
changing anything a person sees, sets or calls, search [docs/product-map.md](docs/product-map.md) for its words (never
read the map or [docs/decisions.md](docs/decisions.md) whole: together they are ~150k tokens). Dated specs, plans and
research are history, never a source for how Faber works today. This file stays short: each line prevents a real
mistake, and a rule a test enforces leaves it.

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
- Every change a person would notice adds a what's-new entry in every language, in the same pull request: one short
  sentence in plain words, written by hand ([how](docs/dev/whats-new.md)), in its own `docs/next/<branch>.json` (never in the locale files, whose
  shared lines conflict). The id only grows, never depends on the version, and is given when the change lands, with
  the decision numbers ([workflow](docs/dev/workflow.md#numbers)). After an update the screen shows what is newer than the
  last entry seen, and the release body is built from the English entries (R142, `scripts/release-notes.mjs`).
- Memory is a feature: Faber with no CLI running stays under 500 MB when complete, and the end-to-end test fails above
  400 MB (R125). The limit is a safety net, not the goal: every change aims at the lightest and most responsive result
  it can reach and reports its memory and processor cost. Anything that keeps a process or a big DOM alive needs a
  reason and a measurement ([architecture](docs/product/architecture.md#architecture)).
- Telemetry stays off by default.
- One pull request per coherent change, the biggest slice one AI does well (one feature or fix with its tests,
  often 300 to 1,000 lines), never a string of 100-line steps of one idea: each pull request costs a queue run. Push
  proven work at once. Lane work enters `phase-1-base` as one squashed pull request that fills
  `.github/pull_request_template.md`, titled as a commit (`pr-title` checks it). Right after opening it, run
  `node scripts/land.mjs <number>` in the background: it runs the suites, squashes and numbers it on `phase-1-base`,
  and CI lands it once every system is green and lets `main` follow (R126, R127, R162). Then go on and never watch CI:
  a red queue run turns the pull request's `queue` check red, which wakes the session; push the complete fix and queue
  it again (R182, [workflow](docs/dev/workflow.md#merge-on-green)). `land.mjs` is the only way in, never the merge
  button or `gh pr merge`. Never merge the base into a branch only to keep up; a conflict only where both sides added
  lines to a `.md` file settles itself at landing (R184).
- A red `phase-1-base` comes first: fix it, or keep your commits until it is green. `cargo t` shows it (`BASE` lines)
  and its `pre-push` hook refuses a push onto it ([workflow](docs/dev/workflow.md#red-base)); `FABER_PUSH_ON_RED=1` pushes
  the fix.
- In a worktree on a branch, claim the areas and files you will change in `.faber-lane` before changing one
  ([workflow](docs/dev/workflow.md#lane-claims)); `cargo t` refuses unclaimed changes and files another lane claims.
- When a task is done (its pull request queued), take the next with `node scripts/tasks.mjs next` and start it without
  asking ([workflow](docs/dev/workflow.md#task-queue)). Only the owner, or the session he names, adds tasks, and the list
  holds at most 15. Ask the owner only what is theirs (direction, taste, money), in a short card, and go on with the
  next task meanwhile; stop when no task is free.
- Until 2026-11-14 the process machinery is frozen (`scripts/land.mjs`, `scripts/tasks.mjs`, `testkit/`, `.github/`):
  change it only to fix a red base or a landing that is blocked, never to improve it
  ([workflow](docs/dev/workflow.md#process-freeze)).
- Never wait idle on a helper, another session or a run: hand out only pieces your next step does not need, keep
  working, and read each result when its notice comes; notices wake the session, so a turn ends only when nothing is
  left (R181). Research and collecting go to a cheap model; reviews go to the other vendor (R146).

## Before a change a person will see or use

Read [docs/product-rules.md](docs/product-rules.md) and search [docs/product-map.md](docs/product-map.md) first, then:

- Look in the product map for what already exists, in either area: the same setting, screen, control, words, layout,
  stored value or shell command. Extend it; a second way to do the same thing is a bug. Check its "Known incoherences"
  before touching those parts.
- Place it with the table in product-rules.md. A setting never lands on a work screen, and a new first-run step needs a
  reason no default covers.
- Check the taste cards whose "Touches" match. The review says "cards checked: C00x ok" or names the line that breaks one.
- Walk it as a newcomer and as a heavy user: first run, no AI connected, empty and waiting states, a 720 px window,
  reduced motion, the longest language. `node scripts/first-user.mjs` has a fresh AI that sees only the screen take a
  first newcomer pass.
- Something that goes against a decision ([docs/decisions.md](docs/decisions.md): search it for the words of the change)
  or a card goes to the owner first.
- How it looks and moves: [docs/design/](docs/design/README.md) (its README and the page of the part you change) and the
  checklist in `.claude/skills/faber-ui/SKILL.md` (Claude loads it as a skill; other agents read the file).

## Done means

- The local proofs below pass, the queued commit's `gate` is green in CI (it lands only then), and a check fails
  without the change (the product map names the check per feature).
- The same commit updates [docs/product-map.md](docs/product-map.md) when a setting, screen, stored value, shell command,
  feature or product word appears, changes or goes (`engine/tests/product_map.rs` fails when one is missing), deletes the
  "Known incoherences" lines it fixed, and updates the [docs/design/](docs/design/README.md) page of a screen whose look
  or behaviour changed. A new decision is a `R-next` row in the branch's `docs/next/<branch>.json`, numbered when it
  lands; a finished spec or plan gets its status in [docs/README.md](docs/README.md). A living page that would pass its
  size limit splits by topic instead (`npm run docs` says so).
- The commit type is a release decision: a `feat`, `fix` or `perf` commit that touches what ships becomes a patch release
  by itself once CI is green (R130); `docs`, `test`, `ci`, `refactor` and `chore` do not.
- For a change a person sees, the commit body says `Extends: <map entry>`, or `New: <the closest existing thing and why
  it does not fit>`.
- The commit body states the memory and processor cost, or why there is none (docs, tests only). CI measures it on
  the queued commit and fails above the R125 limit; a change that can move it names the CI run that measured it.
- A change to engine state, stored data, permissions, processes or security gets a review from a fresh session that
  did not write it: the other vendor when it is available, the same vendor in a new read-only session when its quota is
  poor (R146); security changes get two. Words, docs, tests and screen-only changes land on their proofs. Every review
  follows the review rules of product-rules.md, and a change a person sees also gets its coherence review.

## Stack

- `engine/`: the engine in Rust (tokio). It drives the CLIs, keeps conversations as JSON files and knows nothing
  about windows.
- `host/`: the commands Faber answers, in one table every front end calls (R459): the window, the command line,
  tests. Being filled while the shell moves from Tauri to Electron.
- `src-tauri/`: the Tauri 2 shell. One window, a tray icon, and the fixed list of commands the screen may call.
- `winproc/`: the unsafe process calls (resuming a process created suspended, so it joins its job before it runs).
  The shell denies unsafe code except in its four Windows API files (`awake.rs`, `autostart.rs`, `tray/native.rs`,
  `pane/native.rs`);
  everything else forbids it.
- `ui/`: the screen in SolidJS + TypeScript, built by Vite into `dist/`.
- `tests/`: the screen's checks (`tests/ui/*.check.ts`, run inside a hidden test build) and the recordings the fake CLI
  replays (`tests/fixtures/`). The engine's own tests replay the same fixtures (`engine/tests/`).
- `testkit/`: Faber's own test runner (`cargo t`) and the fake CLI. It runs only the tests that depend on what changed,
  launches a hidden test build inside a sandbox and audits every run ([testing](docs/dev/testing.md#screen-tests)).

## Proof commands

All exit 0 before any commit. None builds the app or waits for a build slot, so they take seconds to a minute. Inside
Faber, run them with Faber's `verify` tool (fast while working, full before the commit), not one by one:

```bash
npm run lint
npm run typecheck
npm run docs
npm run build
cargo fmt --all --check
npm run test:scripts
node scripts/cargo.mjs t --claims
```

`cargo t --claims` checks the lane's claims and shows a red `phase-1-base`; it builds only the runner. The heavy proofs run
once per landing: `land.mjs` runs the suites a change needs on the PC on the exact commit it queues, and CI leaves out
what that covered (R190 V2); otherwise CI runs them on the queued commit, and never on the PC by habit: the `rust`, `ui` and `screen` suites (`cargo t --all`, the
memory limit of R125 included) and the Clippy proofs (`cargo t --suite lint`: `--workspace --all-targets` and `-p faber
--features test-hooks`, each with `-D warnings`, plus macOS on Windows through zig,
[testing](docs/dev/testing.md#what-a-run-prints)). A red one wakes the lane ([workflow](docs/dev/workflow.md#merge-on-green)).
Run a suite on the PC only to see a new check fail before the change (`cargo t --filter <name>`) or to look closer at a
failure CI reported; it runs only the tests that depend on the files you changed and prints only failures, measurements
and a summary (every line in `target/faber-test/run.log`). A command that may take over two minutes (`cargo t`, a
build) runs in the background while you keep working; wait for its notice or the Monitor tool, never a `sleep` or
`until` loop, and never poll CI (the app watches it). `npm run dev` opens the app from source with live reload. Unit
tests never spawn processes, touch real git or call a real CLI.

Local compiling commands go through `node scripts/cargo.mjs <cargo arguments>`: it reuses three caches across worktrees
and cleans owned temporary files. `npm run dev` and `npm run dist` already use it; direct Cargo bypasses the storage
policy. See [testing](docs/dev/testing.md#build-folder-budget).

On the maintainer's PC, Rust lives outside the default folders; export these before any `cargo` or `npm run dev`:

```bash
export RUSTUP_HOME='E:\Apps\rust\rustup' CARGO_HOME='E:\Apps\rust\cargo' PATH="/e/Apps/zig:/e/Apps/rust/cargo/bin:$PATH"
```

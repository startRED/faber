# Faber

**One app for everything AI can do.**

Faber is an open-source desktop app that runs the official Claude Code and Codex command-line tools with the
subscriptions you already pay for. It aims to do anything AI can do, at commercial quality, so you never need another
AI app. It is for anyone: if you use AI every day it feels familiar, and if you never have, you learn it by using it.
It runs on Windows, macOS and Linux, with Windows first.

It has two areas:

- **Chat**: many conversations, no folders, never touches your files. Pick a model or Auto, and always see which model
  answered and, in Auto, why. Each conversation has a hidden workspace where the model runs code and creates files. The
  goal is to beat claude.ai and ChatGPT.
- **Projects**: open a folder, like in Claude Code, and keep many projects with many conversations each.

Faber works with Claude only, Codex only, or both. It supports no other vendor. The subscription is the main path, so
nobody needs an API key. Faber never reads or stores your logins: you sign in through each tool's own flow.

> **Status:** early development. Nothing to install yet. Follow the [roadmap](docs/roadmap.md).

## What works today

- Chat and Projects on Claude Code and Codex: streaming answers, interrupt, resume after reopening, attachments, and
  branching a conversation.
- Auto, which picks the model for each message and shows why.
- A tray icon, five languages and self-updates.

## Planned

The rest of the direction is not built yet. The roadmap says which phase brings each part.

- **A lead agent in Projects** that can do anything AI can do: organize a folder, plan a project with you, build a
  complete app ready to ship, make thumbnails, videos, 3D models, games, pixel art and game mods, or drive the apps on
  your PC. There is no fixed list of skills. For each request it picks the path: do it now, plan it with you, or split
  it across several Claude and Codex workers that build, review and verify the work.
- **Reviews by another model.** The writer never reviews its own work. With both vendors connected, the review comes
  from the other vendor. With one, it comes from a different model of the same vendor in a fresh session, and Faber
  says the review is weaker.
- **Proof before "done".** Every result ships with evidence: tests that ran, a screen recording, or scores from judges
  who did not make the work.
- **Undo.** Faber snapshots files before touching them.
- **Project memory in plain files** inside the project folder, so your work outlives the app.
- **A curated catalog.** Nothing third-party is bundled. Community tools are downloaded from their original source,
  with the license shown first.
- **Spending expiring quota.** Faber routes each task to the right model and spends quota that would otherwise expire.

## Principles

- **Universal, not scripted.** No fixed pipelines per task. The agent decides each step; recipes are learned shortcuts,
  never menus.
- **Nothing third-party is bundled.** This already holds for the app and the repo.
- **Light.** Faber with no CLI running stays under 500 MB when complete. Today it measures 89 MB idle.

## Build from source

You need Node 24 or newer, a stable Rust toolchain and the [Tauri prerequisites](https://tauri.app/start/prerequisites/)
for your system. On Windows that means the C++ build tools and WebView2. On Linux it means:

```bash
sudo apt-get install -y libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev patchelf
```

Then:

```bash
npm ci
npm run dev
```

`npm run dev` opens the app from source with live reload. Before any commit, all of these exit 0:

```bash
npm run lint
npm run typecheck
npm test
npm run build
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

On Windows, `npm run test:e2e` also builds the app and drives it in a hidden window. Run it when the screen or the shell
changes.

## License

[AGPL-3.0-only](LICENSE)

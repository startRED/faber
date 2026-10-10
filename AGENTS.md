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
- Every `child_process` call passes an explicit `maxBuffer` and never uses `shell: true`. On Windows, spawning a
  `.cmd` shim without a shell fails with `EINVAL`: resolve the real executable instead.
- Telemetry stays off by default.
- One commit per coherent change, made as soon as the change is proven.

## Proof commands

All four exit 0 before any commit:

```bash
npm run lint
npm run typecheck
npm test
npm run build
```

Tests use Node's built-in runner (`node --test`) on `.ts` files directly. Unit tests never spawn processes, touch
real git or call a real CLI; those belong to a separate integration tier.

## Layout

- `src/main/`: Electron main process (compiled by `tsc` to `dist/main/`).
- `src/renderer/`: React UI (built by Vite to `dist/renderer/`).
- `tests/`: `*.test.ts`, one file per module.
- `electron-builder.yml` and `.github/workflows/release.yml`: installers published to GitHub Releases on every `v*` tag.

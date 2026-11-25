# Contributing to Faber

Thanks for helping. Faber is maintained by one person, so a small, well-described change is the easiest kind to review.
Everyone taking part follows the [Code of Conduct](CODE_OF_CONDUCT.md).

## Ways to contribute

- **Bug reports.** Use the bug report form when you [open an issue](https://github.com/startRED/faber/issues/new/choose).
  Never paste API keys, sign-in tokens or session credentials.
- **Documentation fixes.** Typos and unclear sentences can go straight to a pull request.
- **Translations.** Faber has five languages in `ui/locales/`. Each is written by hand, never machine-translated, and a
  native speaker reviews each new language. Corrections from native speakers are especially useful.
- **Scoped fixes and screen improvements.** One bug or one improvement per pull request, with a test that fails without it.
- **Catalog candidates.** Suggest a skill or tool server through an issue, with its source, license and why it belongs.
  Faber never bundles third-party tools; the catalog downloads them from their original source.

## Before you write code

- Read the [non-goals](docs/product/README.md#non-goals) so effort does not go to something Faber will not do.
- Open an issue first for a new feature or a large change. Small bug fixes and documentation typos can skip it.
- [docs/README.md](docs/README.md) says which living page answers which question: read the page for the area you touch,
  not a whole folder. For a change a person sees, also read [product-rules.md](docs/product-rules.md) and search
  [product-map.md](docs/product-map.md) for the words of your change.

## Development setup

You need Node 24 or newer, a stable Rust toolchain and the [Tauri prerequisites](https://tauri.app/start/prerequisites/)
for your system. Then:

```bash
npm ci
npm run dev
```

The [README](README.md#build-from-source) has the Linux packages and the full list of proof commands.

## Pull requests

1. Fork the repository and branch from `phase-1-base`. Pull requests target `phase-1-base`; `main` follows it
   automatically once a commit there is green.
2. Title the pull request `type(scope): summary`, where type is one of `feat`, `fix`, `perf`, `docs`, `test`, `ci`,
   `refactor`, `chore`, `style`, `build` or `revert`. The `pr-title` check enforces it. A `feat`, `fix` or `perf` change
   to the shipped app becomes a preview release once it lands.
3. Run the checks and make sure they all exit 0:

   ```bash
   npm run lint && npm run typecheck && npm run docs && npm run build && cargo fmt --all --check && npm run test:scripts && node scripts/cargo.mjs t --claims
   ```

   `node scripts/cargo.mjs t` also runs the tests your changes affect. The heavy suites and Clippy run in CI.
4. Fill in the pull request template. Your fork's pull request runs the checks independently. A maintainer reviews it,
   and an approved one is squashed into a single commit on `phase-1-base`. You do not need `scripts/ship.mjs` or
   `scripts/land.mjs`; those are for the maintainer's own workflow.

A change is done when a check fails without it. When it changes what a person sees, update
[product-map.md](docs/product-map.md) and the matching page under `docs/design/` in the same pull request.

## Project rules

- Code, comments, docs, commit messages and pull request titles are in English. Contribute original code only.
- No vendor names in product, feature or package names.
- Never read, copy, store or forward CLI credentials or session tokens.
- Never bundle third-party tools, skills, models or MCP servers.
- Child processes never go through a shell. Unit tests never spawn real processes, touch real git or call a real CLI.
- Every text on screen lives in `ui/locales/`; English (`en.ts`) is the source.
- Living pages under `docs/` stay within 240 lines and 20 KB, are listed in [docs/README.md](docs/README.md) and have
  working links. `npm run docs` checks this.
- Memory is a feature. With no CLI running Faber stays under 500 MB, and the end-to-end test fails above 400 MB. A change
  to processes, long-lived state or how much the screen keeps drawn states its memory and processor cost in the pull
  request.

## Security

Report vulnerabilities privately as [SECURITY.md](SECURITY.md) describes. Do not open a public issue or pull request
for one.

## License

Faber is source-available under [FSL-1.1-ALv2](LICENSE); see also [NOTICE](NOTICE). Read both before you contribute.

## Working with AI agents and the maintainer workflow

[AGENTS.md](AGENTS.md) holds the rules for AI agents working in this repository, and
[docs/dev/workflow.md](docs/dev/workflow.md) describes how the maintainer organizes work.

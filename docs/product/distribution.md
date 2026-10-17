# License and distribution

The license, the providers' terms and how Faber is distributed.

## License

Faber is under the Functional Source License, Version 1.1, ALv2 Future License (`LICENSE`, SPDX `FSL-1.1-ALv2`).
Faber is source-available, not open source. Anyone may use, change and share it for any purpose except a Competing Use,
as defined in `LICENSE`. Each version becomes Apache 2.0 two years after its release. What users create with Faber
belongs to them.

## Provider terms

- Anthropic allows a user to sign in to the unmodified Claude Code binary with their own subscription, including
  inside another platform. Faber accepts Anthropic's Commercial Terms before shipping and never offers a claude.ai
  login of its own.
- OpenAI: Codex runs through its official CLI and app-server with the user's own login. Faber asks OpenAI for written
  confirmation.
- Faber never drives a chat website (ChatGPT, Claude) in a browser or copies its login session: that breaks the
  providers' terms and puts the user's account at risk.

## Distribution

- GitHub Releases on every tag, downloaded directly: an `.exe` (NSIS) installer for Windows, a universal `.dmg` for macOS,
  AppImage and `.deb` for Linux. Windows comes first, and macOS and Linux compatibility is kept from now on. arm64
  Windows and `.rpm` come later.
- Every user-facing change ships by itself as a preview, and a stable follows weekly (R130, R156). When CI goes green on
  `phase-1-base` and a `feat`, `fix` or `perf` commit that touches the shipped app (`ui/`, `engine/`, `src-tauri/`,
  `package.json`, `vite.config.ts`) landed since the last tag, `auto-release.yml` tags `vX.Y.(Z+1)-preview.N` and starts the
  release as a GitHub pre-release. People who turned on "Receive previews" update to it through the built-in updater, from
  the fixed `preview-channel` release's `latest.json`. Every Wednesday `promote-stable.yml` rebuilds the newest preview
  published at least a day ago (with a complete manifest) as `vX.Y.Z`, the release `/releases/latest` points at, which
  everyone else reads (it can also be run by hand with a preview tag for an urgent fix). A finished area still gets its
  minor bump by hand. The tag is the version; the
  release build writes it into `package.json`. Semver pre-releases are accepted by every bundle (NSIS, DMG, AppImage,
  deb); an MSI would reject them and is not built. Every release carries `SHA256SUMS.txt` and build provenance
  (`gh attestation verify <file> --repo startRED/faber`). `scripts/channel.mjs` holds the rules and its tests run in CI.
- No paid signing for now. Windows shows a SmartScreen notice that the download page explains. The Microsoft Store and
  a code-signing certificate come back to the table when everything is complete.
- macOS builds are not notarized, so users allow the app once in System Settings. Faber updates itself there too. If a
  hand test on a real Mac shows that an unsigned update cannot install, macOS falls back to a notification about a new
  version.
- Later: winget and Flathub.

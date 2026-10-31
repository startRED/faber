# The README says how to install Faber, and the living pages stop claiming what is not true

Area: Preview, stable
Files: README.md, docs/product/distribution.md, docs/product/architecture.md, scripts/docs.mjs, scripts/docs.test.mjs
Depends: none
Size: about 80 changed lines

Why: weekly stable releases exist (GitHub Releases, Windows `.exe`, macOS `.dmg`, Linux AppImage and `.deb`), but the first
thing a newcomer reads, `README.md:20`, still says "Status: early development. Nothing to install yet." The README gives
newcomers no download link or installation steps, and `docs/product/distribution.md:38` says "Windows shows a SmartScreen notice that the download page
explains" although no download page exists; the macOS "allow the app once in System Settings" step (`distribution.md:40`)
is written nowhere a user reads it. Two lines of `docs/product/architecture.md` (the Updates section, about line 79-82)
contradict the code: "Installing when Faber opens (R141) is not built yet", while `src-tauri/src/update.rs` has the open-time
check and `docs/product-map.md` documents the "Update opening screen"; and "Claude Code and Codex move only to versions that
GitHub tested overnight", while `engine/src/install.rs` installs the newest release and the live manifest that would hold a
version back is not built (`docs/product-map.md`, "Not built yet").

Build:
- `README.md`: replace the status note with a short "Install" section: the link to the newest release
  (`https://github.com/startRED/faber/releases/latest`), which file to take per system, the SmartScreen step on Windows
  ("More info", then "Run anyway") and the macOS step (System Settings > Privacy & Security > "Open Anyway"), and that
  Faber updates itself afterwards. Keep the board and roadmap links. Plain words, no promise of a date or a version.
- `distribution.md`: the SmartScreen and macOS lines point at that README section instead of a download page.
- `architecture.md`: the two lines say what is true today: Faber checks on open and installs a found update before work starts when it can, otherwise
  it keeps the deferred install (`update.rs`, its opening comment); the CLIs Faber
  installs are their newest release, and holding one back to a tested version comes with the live manifest (link the spec
  `docs/superpowers/specs/2026-10-24-cli-tools-live-manifest.md`).
- `npm run docs` (`scripts/docs.mjs`) fails when `README.md` says "Nothing to install" or lacks a link to
  `/releases/latest`, with its test in `scripts/docs.test.mjs`.

Leave out: a website or GitHub Pages, winget or the Store, any change to the release workflow or the updater, macOS
behaviour changes (only the words).

Done: the new docs test fails on today's README and passes after; `npm run docs` and `npm run test:scripts` pass. Extends:
"Preview, stable" in `docs/product-map.md` (words only, the row itself does not change). What's-new: none (no screen
change). Review: a fresh read-only session reads the README section as a newcomer on Windows and on macOS. Cost: none
(docs and a docs check).

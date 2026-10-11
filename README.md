# Faber

**One app for everything AI can do.**

Faber is an open-source desktop app for Windows, macOS and Linux with two areas:

- **Chat**: many conversations, no folders, never touches your files. Pick any model from the plans you have, or
  Auto, and always see which model answered and why. It adds a second opinion from another vendor, sources checked by a different model,
  deep research with several models in parallel, branching conversations, editable memory and sandboxed data analysis.
- **Projects**: open a folder, like in Claude Code, and talk to a lead agent that can do anything AI can do: organize
  a folder, plan a project with you, build a complete app ready to ship, make thumbnails, videos, 3D models, games,
  pixel art and game mods, or drive the apps on your PC. There is no fixed list of skills. The lead agent coordinates
  models from different vendors that build, review and verify the work.

Faber runs the official AI command-line tools (Claude Code, Codex and others) with whichever subscriptions you
already pay for, one or several. No API key is required, and Faber never reads or stores your logins.

> **Status:** early development. Nothing to install yet. Follow the [roadmap](docs/roadmap.md).

## Principles

- **Universal, not scripted.** No fixed pipelines per task. The lead agent decides each step; recipes are learned
  shortcuts, never menus.
- **Proof before "done".** Every result ships with evidence: tests that ran, a screen recording, or scores from two
  judges from different vendors.
- **The writer never reviews its own code.** Reviews come from a model of another vendor, in a fresh session.
- **Everything local is undoable.** Faber snapshots files before touching them.
- **Your work outlives the app.** Projects live in an open, versioned folder format.
- **Nothing third-party is bundled.** Community tools come from a curated catalog and are downloaded from their
  original source, with the license shown first.
- **Quota is precious.** Faber routes each task to the right model and spends quota that would otherwise expire.

## License

[AGPL-3.0-only](LICENSE)

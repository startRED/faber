# Let a project's manifest facts guide which tools a session gets

Area: Session kits
Files: engine/src/tools/kits/matching.rs, engine/src/tools/kits.rs, engine/src/project/rules.rs, engine/tests/kits.rs
Depends: none
Size: about 250 changed lines

Why: the spec's "Tasks and tool kits" (`docs/superpowers/specs/2026-10-22-own-work-method.md`) says the method "only
feeds that matcher with `rules.md`'s stack facts, and slice 1 checks that a project's kit is unchanged or better with
them". `stack_labels(request, cwd)` in `engine/src/tools/kits/matching.rs` decides the stack from the request's words
and from six file names in the working folder (`Package.swift`, `pubspec.yaml`, `schema.prisma`, `wrangler.toml`...).
`rules::Facts` (`languages`, `manifests`) is read by nothing there, so a project whose `package.json` names `expo` or
`three` gets those kits only when the request says the word.

Build:

- Extend `stack_labels` with labels from manifest facts of a fixed shape: dependency names in `package.json`,
  `Cargo.toml` and `pyproject.toml` that equal a label the matcher already knows (`expo`, `react-native`, `threejs`,
  `phaser`, `pixijs`, `prisma`, `bevy`...), read through the same no-follow handles as `rules::facts`. A name that is
  not on the matcher's list is ignored; no repository free text is read.
- A label only widens which stack-specific items may match; it never adds an item the task words do not ask for.
- Keep the cost: one manifest read per session start, shared with the rules rebuild, no new process.

Leave out: changing the catalog or the job words, and any new kit setting.

Done: engine tests in `engine/tests/kits.rs` that fail today show a project whose manifest names a known stack gets the
stack's kit without the word in the request, a request on a project with no manifest facts gets exactly the kit it got
before, and a hostile manifest cannot add a label that is not on the list. The commit body states time per session
start. Extends: the product-map section "Session kits". Review by the other vendor (R146).

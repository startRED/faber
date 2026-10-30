# Look again at plugin candidates under the delivery preflight

Area: Catalog
Files: docs/research/2026-10-21/catalog-list.md, engine/src/tools.json, engine/tests/plugins.rs, tests/ui/plugins.check.ts
Depends: none
Size: about 200 changed lines

Why: the catalog has checked plugin delivery (`engine/src/tools/plugins.rs`, `plugin_delivery.rs`) but no plugin ships:
the twenty original candidates were all excluded, each with its own evidence in
`docs/research/2026-10-21/catalog-list.md` ("Plugin delivery preflight"). The owner wants plugins in the first stable
catalog (2026-10-23). A plugin groups checked skills, slash-command prompts, subagent definitions and R253-portable npm
servers under one catalog id; hooks, binaries, postinstall, dependencies, unsupported delivery and unchecked parts keep
the whole candidate out, and a rejection is never reversed by packaging alone.

Build:

- Look again at plugins published since the audit, and at the twenty rejected ones whose blocking part has since changed
  at the original source (a new commit that drops the hook or the dependency). Apply the preflight as written, part by
  part, with the pinned commit and whole-folder digest of the original source.
- For a plugin whose every part passes: add it to the catalog data with its pins, licence and part list, and a lifecycle
  check (install, turn on and off, Undo, removal) in `engine/tests/plugins.rs`; the Plugins filter then shows it.
- Record every candidate looked at with the reason, kept or left out, in the research note. If none passes, the note
  says so and nothing else changes.

Leave out: any change to how plugins are delivered, a plugin that needs hosted configuration, and bundling.

Done: the research note lists each candidate's reason; for each plugin kept, the lifecycle check fails without its
catalog entry, and `tests/ui/plugins.check.ts` shows its card with parts and access limits. Extends: the product-map rows
"Catalog" and "Portable catalog plugins". What's-new entry in every language when a plugin ships. Two reviews, one by the
other vendor, because a plugin brings instructions and servers (R146).

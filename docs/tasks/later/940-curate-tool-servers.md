# Add the tool servers a software project uses every day to the catalog

Area: Catalog
Files: engine/src/tools.json, engine/tests/tools.rs, engine/tests/kits.rs, docs/research/2026-10-21/catalog-list.md, tests/ui/catalog.check.ts
Depends: none
Size: about 300 changed lines

Why: the owner wants the first stable version to have a catalog of skills, plugins and tool servers that install,
uninstall, turn on and off, and that sessions pick by themselves (2026-10-23). The catalog holds 114 skills and one
tool server (`chrome-devtools` in `engine/src/tools.json`), and no plugins (product map, "Catalog tab").
`docs/research/2026-10-21/catalog-list.md` says why each candidate was kept or left out.

Build:

- Research and curate tool servers a software project uses every day (browser automation beyond the one kept, the code
  host, docs lookup, databases and the like). A server enters only under R253: an npm package with no dependencies,
  pinned to a version with the digest of its files, from registry.npmjs.org, free to use with no key Faber must hold and
  no account Faber would have to store, with its licence shown. Anything that needs a paid key or an account is out.
- Each kept server gets its entry in `tools.json` (`id`, `job`, `license`, `repo`, `commit`, `digest`, and `server`
  with package, version, entry, args and env, as `chrome-devtools` has them), with the same containment and refusal rules
  as the kept server (no telemetry, no update checks, no file navigation where the package has such switches). In Chat
  a server declares that it touches no PC files and needs no credentials, or it is left out of Chat (R373).
- Write each candidate's reason, kept or left out, in the research note, like the other candidates. If no candidate
  passes, the note says so and the task still ends there: an empty answer is a result.
- Never bundle anything (AGENTS.md): the catalog lists, the install downloads from the source.

Leave out: plugins (task recheck-plugin-candidates), servers that need a key, and any change of the delivery code.

Done: for each kept server, a check with the fake source (`engine/tests/tools.rs`) that installs it, starts it and
removes it, a session-kit check in `engine/tests/kits.rs` that a task that needs it picks it, and the catalog count test
and `tests/ui/catalog.check.ts` updated; the research note lists the reasons. Extends: the product-map rows "Catalog" and
"Session kits". What's-new entry in every language. Two reviews, one by the other vendor, because a server runs code
(R146).
